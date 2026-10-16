//! Account-scoped immutable original bundles. No network until an account is linked.
use anyhow::{Context,Result};
use serde::{Deserialize,Serialize};
use std::{collections::BTreeMap,fs,io::{Cursor,Read,Write},path::Path};
const LIMIT:usize=100*1024*1024;
const MAX_FILES:usize=512;
#[derive(Clone,Serialize,Deserialize)] pub struct Session { pub server:String, pub access_token:String, pub user_id:String }
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct VaultOwner { version:u32,server:String,user_id:String }
#[derive(Debug)] pub struct DifferentVaultOwner;
impl std::fmt::Display for DifferentVaultOwner { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str("vault_owner_mismatch") } }
impl std::error::Error for DifferentVaultOwner {}
pub fn account_failure_message(error:&anyhow::Error)->&'static str {
    if error.downcast_ref::<DifferentVaultOwner>().is_some() { "Este archivo pertenece a otra cuenta o servicio. No se modificó tu sesión ni se subieron copias; utiliza un perfil Windows separado." }
    else { "No se pudo vincular la cuenta. Revisa servicio, conexión y datos; la sesión anterior se conserva." }
}
#[derive(Serialize,Deserialize)] #[serde(deny_unknown_fields)] struct Record { size:usize,sha256:String }
#[derive(Serialize,Deserialize)] #[serde(deny_unknown_fields)] struct Manifest { schema:u32,udid:String,card_hash:String,files:BTreeMap<String,Record> }
#[derive(Serialize,Deserialize)] #[serde(deny_unknown_fields)] struct Receipt { version:u32,bundle_id:String,manifest:Manifest }

fn normalized_server(value:&str)->Result<String> {
    anyhow::ensure!(value.is_ascii() && value.len()<=2048 && !value.contains(['@','?','#','\\']) && !value.bytes().any(|b|b<=0x20 || b==0x7f),"invalid cloud service URL");
    let (scheme,rest)=value.split_once("://").context("cloud URL scheme missing")?;
    anyhow::ensure!(matches!(scheme,"https"|"http"),"cloud service needs HTTPS");
    let (authority,path)=rest.split_once('/').map(|(host,path)|(host,format!("/{path}"))).unwrap_or((rest,String::new()));
    let (host,port)=authority.split_once(':').map(|(host,port)|(host,Some(port))).unwrap_or((authority,None));
    let host=host.to_ascii_lowercase();
    anyhow::ensure!(!host.is_empty() && host.len()<=253 && host.split('.').all(|label|!label.is_empty() && label.len()<=63 && !label.starts_with('-') && !label.ends_with('-') && label.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-')),"cloud host invalid");
    anyhow::ensure!(scheme=="https" || host=="127.0.0.1","only localhost may use HTTP");
    let port=port.map(|text|text.parse::<u16>().context("cloud port invalid")).transpose()?;
    anyhow::ensure!(port.is_none_or(|p|p>0) && path.split('/').all(|part|part!="." && part!=".." && part.bytes().all(|b|b.is_ascii_alphanumeric() || matches!(b,b'-'|b'_'|b'.'))),"cloud URL path invalid");
    let port=port.filter(|p|*p!=if scheme=="https" {443} else {80});
    Ok(format!("{scheme}://{host}{}{}",port.map(|p|format!(":{p}")).unwrap_or_default(),path.trim_end_matches('/')))
}
pub fn valid_server(url:&str)->bool { normalized_server(url).is_ok() }

fn owner_for(session:&Session)->Result<VaultOwner> {
    anyhow::ensure!(!session.user_id.is_empty() && session.user_id.len()<=64 && session.user_id.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-'),"cloud owner identity invalid");
    Ok(VaultOwner { version:1,server:normalized_server(&session.server)?,user_id:session.user_id.clone() })
}
fn load_owner_at(root:&Path)->Result<Option<VaultOwner>> {
    let path=root.join("vault-owner.json"); crate::storage::safe_chain(&path)?;
    let metadata=match fs::symlink_metadata(&path) { Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(None),result=>result? };
    anyhow::ensure!(metadata.is_file() && metadata.len()<=4096,"vault owner metadata invalid");
    let owner:VaultOwner=serde_json::from_slice(&fs::read(&path)?)?;
    anyhow::ensure!(owner.version==1 && owner.server==normalized_server(&owner.server)? && !owner.user_id.is_empty() && owner.user_id.len()<=64 && owner.user_id.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-'),"vault owner invalid"); Ok(Some(owner))
}
fn require_owner_at(root:&Path,session:&Session)->Result<VaultOwner> {
    let expected=owner_for(session)?;
    anyhow::ensure!(load_owner_at(root)?.as_ref()==Some(&expected),"Esta instalación pertenece a otra cuenta o requiere vinculación explícita; no se sincronizarán copias"); Ok(expected)
}
fn bind_owner_at(root:&Path,session:&Session,initial_link_consent:bool)->Result<()> {
    let expected=owner_for(session)?; crate::storage::safe_chain(root)?; fs::create_dir_all(root)?;
    let lock=root.join(".owner-link.lock"); let handle=fs::OpenOptions::new().write(true).create_new(true).open(&lock).context("Otra cuenta se está vinculando o necesita revisión")?;
    let result=(|| { match load_owner_at(root)? {
        Some(owner)=>if owner!=expected { return Err(DifferentVaultOwner.into()); },
        None=> { anyhow::ensure!(initial_link_consent,"Confirma la vinculación de todas las copias locales antes de iniciar sesión"); crate::storage::write_new(&root.join("vault-owner.json"),&serde_json::to_vec(&expected)?)?; }
    } Ok(()) })();
    drop(handle); if result.is_ok() || load_owner_at(root).is_ok() { fs::remove_file(lock)?; } result
}
fn seen_directory(root:&Path,owner:&VaultOwner)->Result<std::path::PathBuf> {
    Ok(root.join("cloud-seen").join(crate::bank_backup::sha256(&serde_json::to_vec(owner)?)?))
}
fn local_manifest_matches(root:&Path,manifest:&Manifest)->Result<bool> {
    if manifest.schema!=1 || manifest.udid.is_empty() || manifest.udid.len()>256 || !crate::wallet_export::is_exact_card_hash(&manifest.card_hash)
        || manifest.files.is_empty() || manifest.files.len()>MAX_FILES {return Ok(false);}
    let mut bank_key=None;let mut total=0usize;
    for (name,record) in &manifest.files {
        if !safe_name(name) || !evidence_name(name) || record.size>32*1024*1024 || record.sha256.len()!=64 || !record.sha256.bytes().all(|b|b.is_ascii_hexdigit()) {return Ok(false);}
        total=total.checked_add(record.size).context("receipt size overflow")?;if total>LIMIT {return Ok(false);}
        let (prefix,tail)=name.split_once('/').context("receipt path invalid")?;
        if prefix=="bank" {
            let key=tail.split('/').next().unwrap();
            if bank_key.as_ref().is_some_and(|previous:&String|previous!=key) {return Ok(false);}bank_key=Some(key.to_string());
        }
        let path=root.join(if prefix=="bank" {"bank-backups"} else {"bank-change-intents"}).join(tail);
        if crate::storage::safe_chain(&path).is_err() {return Ok(false);}
        let metadata=match fs::symlink_metadata(&path) {Ok(metadata)=>metadata,Err(_)=>return Ok(false)};
        if !metadata.is_file() || metadata.len()!=record.size as u64 {return Ok(false);}
        if crate::bank_backup::sha256(&fs::read(&path)?)?!=record.sha256 {return Ok(false);}
    }
    let Some(key)=bank_key else {return Ok(false);};
    if !["baseline.json","manifest.urls","remote-artwork.bin"].iter().all(|leaf|manifest.files.contains_key(&format!("bank/{key}/{leaf}"))) {return Ok(false);}
    let baseline=match crate::bank_backup::validate_bundle_directory(&root.join("bank-backups").join(key)) {Ok(baseline)=>baseline,Err(_)=>return Ok(false)};
    if baseline.udid!=manifest.udid || baseline.card_hash!=manifest.card_hash || crate::storage::ensure_card_ready_at(root,&manifest.udid,&manifest.card_hash).is_err() {return Ok(false);}
    // Extra immutable revisions are allowed, but the current complete chain
    // and every revision declared by this older snapshot must still be intact.
    Ok(crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"),&manifest.udid,&manifest.card_hash).is_ok())
}
fn verified_seen(root:&Path,path:&Path,id:&str)->Result<bool> {
    let _lease=crate::storage::operation_lease_for_import(root)?;
    crate::storage::safe_chain(path)?;
    let metadata=match fs::symlink_metadata(path) {Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(false),result=>result?};
    anyhow::ensure!(metadata.is_file(),"cloud receipt invalid");if metadata.len()>1024*1024 {return Ok(false);}
    let bytes=fs::read(path)?;
    // v0 receipts attest only a past download, never today's local files.
    if bytes==b"verified" {return Ok(false);}
    let receipt:Receipt=match serde_json::from_slice(&bytes) {Ok(receipt)=>receipt,Err(_)=>return Ok(false)};
    if receipt.version!=1 || receipt.bundle_id!=id {return Ok(false);}
    local_manifest_matches(root,&receipt.manifest)
}
fn write_seen(root:&Path,path:&Path,id:&str,manifest:Manifest)->Result<()> {
    let _lease=crate::storage::operation_lease_for_import(root)?;
    anyhow::ensure!(local_manifest_matches(root,&manifest)?,"local evidence changed before recording cloud receipt");
    let bytes=serde_json::to_vec(&Receipt {version:1,bundle_id:id.into(),manifest})?;
    anyhow::ensure!(bytes.len()<=1024*1024,"cloud receipt too large");
    // Receipt metadata is mutable cache; original/history files stay create-only.
    crate::storage::write_mutable(path,&bytes)
}
fn safe_name(name:&str)->bool {
    !name.is_empty() && name.len()<256 && !name.contains(['\\',':']) && !name.starts_with('/') &&
        name.split('/').all(|part| !part.is_empty() && part!="." && part!=".." && part.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c,b'-'|b'_'|b'.'|b'@')))
}
fn evidence_name(name:&str)->bool {
    let parts:Vec<_>=name.split('/').collect(); if parts.len()!=3 { return false; }
    let leaf=parts[2]; let asset=crate::wallet_backup::CARD_ARTWORK_ASSETS.contains(&leaf);
    match parts[0] { "bank"=>asset || ["baseline.json","manifest.urls","remote-artwork.bin"].contains(&leaf),
        "intents"=>asset || regex::Regex::new(r"^state-[0-9]{4,}-(prepared|applied|originals_verified|restored)\.json$").unwrap().is_match(leaf), _=>false }
}
fn files_in(directory:&Path,prefix:&str,files:&mut BTreeMap<String,Vec<u8>>)->Result<()> {
    for entry in fs::read_dir(directory)? { let path=entry?.path(); let meta=fs::symlink_metadata(&path)?;
        crate::storage::safe_chain(&path)?;
        anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=32*1024*1024,"unsafe cloud evidence file");
        let name=format!("{prefix}/{}",path.file_name().context("missing leaf")?.to_string_lossy());
        anyhow::ensure!(safe_name(&name) && evidence_name(&name) && files.len()<MAX_FILES,"cloud evidence name/count invalid");
        files.insert(name,fs::read(path)?);
    } Ok(())
}

pub fn export_bundle(root:&Path,baseline_directory:&Path)->Result<Vec<u8>> {
    let _lease=crate::storage::operation_lease_for_import(root).context(LocalReview::Busy)?;
    let baseline=crate::bank_backup::validate_bundle_directory(baseline_directory).context(LocalReview::Original)?;
    crate::storage::ensure_card_ready_at(root,&baseline.udid,&baseline.card_hash).context(LocalReview::Conflict)?;
    let pending=crate::wallet_export::pending_recovery_operations_at(&root.join("wallet-export-recovery")).context(LocalReview::Recovery)?;
    anyhow::ensure!(!crate::wallet_export::pending_recovery_blocks_device(Ok(&pending),Some(&baseline.udid)),LocalReview::Recovery);
    let key=baseline_directory.file_name().context("missing identity key")?.to_string_lossy();
    let mut files=BTreeMap::new(); files_in(baseline_directory,&format!("bank/{key}"),&mut files)?;
    for operation in crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"),&baseline.udid,&baseline.card_hash).context(LocalReview::History)? {
        files_in(&operation,&format!("intents/{}",operation.file_name().unwrap().to_string_lossy()),&mut files)?;
    }
    anyhow::ensure!(files.values().map(Vec::len).sum::<usize>()<LIMIT,"cloud bundle too large");
    let records=files.iter().map(|(name,bytes)|Ok((name.clone(),Record { size:bytes.len(),sha256:crate::bank_backup::sha256(bytes)? }))).collect::<Result<_>>()?;
    let manifest=Manifest { schema:1,udid:baseline.udid,card_hash:baseline.card_hash,files:records };
    let mut archive=zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).unix_permissions(0o600);
    archive.start_file("bundle.json",options)?; archive.write_all(&serde_json::to_vec(&manifest)?)?;
    for (name,bytes) in files { archive.start_file(name,options)?; archive.write_all(&bytes)?; }
    let bytes=archive.finish()?.into_inner(); anyhow::ensure!(bytes.len()<=LIMIT,"cloud archive too large"); Ok(bytes)
}

fn parse_bundle(bytes:&[u8])->Result<(Manifest,BTreeMap<String,Vec<u8>>)> {
    anyhow::ensure!(bytes.len()<=LIMIT,"cloud input too large");
    let mut archive=zip::ZipArchive::new(Cursor::new(bytes))?;
    anyhow::ensure!(archive.len()>1 && archive.len()<=MAX_FILES+1,"cloud entry count invalid");
    let mut files=BTreeMap::new(); let mut total=0usize;
    for index in 0..archive.len() { let item=archive.by_index(index)?; let name=item.name().to_string();
        anyhow::ensure!(safe_name(&name) && !item.is_dir() && item.unix_mode().is_none_or(|mode|mode&0o170000!=0o120000),"unsafe cloud archive entry");
        let size=usize::try_from(item.size())?; total=total.checked_add(size).context("expanded size overflow")?;
        anyhow::ensure!(size<=32*1024*1024 && total<=LIMIT,"cloud expanded data too large");
        let mut data=Vec::new(); item.take(size as u64+1).read_to_end(&mut data)?;
        anyhow::ensure!(data.len()==size && files.insert(name,data).is_none(),"duplicate or malformed cloud entry");
    }
    let metadata=files.remove("bundle.json").context("cloud manifest missing")?;
    anyhow::ensure!(metadata.len()<=1024*1024,"cloud metadata too large");
    let manifest:Manifest=serde_json::from_slice(&metadata)?;
    anyhow::ensure!(manifest.schema==1 && crate::wallet_export::is_exact_card_hash(&manifest.card_hash) && !manifest.udid.is_empty() && manifest.udid.len()<=256,"cloud identity invalid");
    anyhow::ensure!(manifest.files.len()==files.len(),"cloud manifest entry mismatch");
    for (name,bytes) in &files { let record=manifest.files.get(name).context("undeclared cloud evidence")?;
        anyhow::ensure!(record.size==bytes.len() && record.sha256==crate::bank_backup::sha256(bytes)?,"cloud evidence checksum mismatch");
        anyhow::ensure!(evidence_name(name),"unsupported cloud content");
    } Ok((manifest,files))
}

pub fn import_bundle(root:&Path,bytes:&[u8])->Result<()> {
    let _lease=crate::storage::operation_lease_for_import(root)?;
    let digest=crate::bank_backup::sha256(bytes)?;
    let (manifest,files)=parse_bundle(bytes)?;
    crate::storage::safe_chain(root)?; fs::create_dir_all(root)?;
    let lock=root.join(".cloud-import.lock");
    let handle=fs::OpenOptions::new().write(true).create_new(true).open(&lock).context("cloud import needs recovery or another process owns it")?;
    let stage=root.join("cloud-staging").join(&digest); crate::storage::safe_chain(&stage)?; fs::create_dir_all(&stage)?;
    let mut verified_identity=None;
    let result=(|| {
        let mut bank_key=None;
        for (name,data) in &files {
            let parts:Vec<_>=name.split('/').collect(); anyhow::ensure!(parts.len()==3,"cloud path depth invalid");
            if parts[0]=="bank" { if let Some(key)=bank_key.as_ref() { anyhow::ensure!(key==parts[1],"mixed baseline identities"); } else { bank_key=Some(parts[1].to_string()); } }
            let staged=stage.join(name); crate::storage::safe_chain(&staged)?; if staged.exists() { anyhow::ensure!(fs::read(&staged)?==*data,"staging evidence conflict"); } else { crate::storage::write_new(&staged,data)?; }
        }
        let key=bank_key.context("cloud original missing")?;
        let baseline=crate::bank_backup::validate_bundle_directory(&stage.join("bank").join(&key))?;
        anyhow::ensure!(baseline.udid==manifest.udid && baseline.card_hash==manifest.card_hash,"cloud baseline identity mismatch");
        verified_identity=Some((baseline.udid.clone(),baseline.card_hash.clone()));
        let staged_intents=stage.join("intents"); fs::create_dir_all(&staged_intents)?;
        let mut merged_bytes=files.values().map(Vec::len).sum::<usize>(); let mut merged_files=files.len();
        for existing in crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"),&manifest.udid,&manifest.card_hash)? {
            let name=existing.file_name().unwrap().to_string_lossy(); let mut existing_files=BTreeMap::new(); files_in(&existing,&format!("intents/{name}"),&mut existing_files)?;
            for (relative,data) in existing_files { let target=stage.join(relative);crate::storage::safe_chain(&target)?;
                if target.exists() { anyhow::ensure!(fs::read(&target)?==data,"conflicting cloud/local intent"); }
                else { merged_bytes=merged_bytes.checked_add(data.len()).context("merged size overflow")?; merged_files+=1;
                    anyhow::ensure!(merged_bytes<=LIMIT && merged_files<=MAX_FILES,"merged history too large; preserve local evidence");
                    crate::storage::write_new(&target,&data)?; }
            }
        }
        crate::bank_changes::validate_cloud_chain(&staged_intents,&manifest.udid,&manifest.card_hash)?;
        for (name,data) in &files {
            let (prefix,tail)=name.split_once('/').unwrap();
            let destination=root.join(if prefix=="bank" {"bank-backups"} else {"bank-change-intents"}).join(tail);
            crate::storage::safe_chain(&destination)?;
            if destination.exists() { anyhow::ensure!(fs::read(&destination)?==*data,"conflicting original evidence"); }
        }
        for (name,data) in &files {
            let (prefix,tail)=name.split_once('/').unwrap(); let destination=root.join(if prefix=="bank" {"bank-backups"} else {"bank-change-intents"}).join(tail);
            if !destination.exists() { crate::storage::write_new(&destination,data)?; }
        }
        Ok(())
    })();
    drop(handle);
    if result.is_ok() { fs::remove_file(lock)?; }
    else { let quarantine=root.join("quarantine").join(format!("{digest}.zip")); if !quarantine.exists() { crate::storage::write_new(&quarantine,bytes)?; }
        if let Some((udid,hash))=verified_identity {crate::storage::block_card_at(root,&udid,&hash)?;fs::remove_file(lock)?;}
        else {crate::storage::block(root)?;}
    }
    result
}

#[cfg(windows)] fn protect(bytes:&[u8],decrypt:bool)->Result<Vec<u8>> {
    #[repr(C)] struct Blob { length:u32,data:*mut u8 }
    #[link(name="crypt32")] unsafe extern "system" {
        fn CryptProtectData(input:*mut Blob,description:*const u16,entropy:*mut Blob,reserved:*mut std::ffi::c_void,prompt:*mut std::ffi::c_void,flags:u32,output:*mut Blob)->i32;
        fn CryptUnprotectData(input:*mut Blob,description:*mut *mut u16,entropy:*mut Blob,reserved:*mut std::ffi::c_void,prompt:*mut std::ffi::c_void,flags:u32,output:*mut Blob)->i32;
    }
    #[link(name="kernel32")] unsafe extern "system" { fn LocalFree(pointer:*mut std::ffi::c_void)->*mut std::ffi::c_void; }
    anyhow::ensure!(bytes.len()<=65536,"account data too large");
    let mut input=Blob { length:bytes.len() as u32,data:bytes.as_ptr() as *mut u8 }; let mut output=Blob { length:0,data:std::ptr::null_mut() };
    let success=unsafe { if decrypt { CryptUnprotectData(&mut input,std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),1,&mut output) }
        else { CryptProtectData(&mut input,std::ptr::null(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut(),1,&mut output) } };
    anyhow::ensure!(success!=0,"No se pudo proteger la sesión de este usuario Windows");
    let result=if output.length<=65536 { Ok(unsafe { std::slice::from_raw_parts(output.data,output.length as usize) }.to_vec()) } else { Err(anyhow::anyhow!("account result too large")) };
    unsafe { LocalFree(output.data.cast()); } result
}
#[cfg(not(windows))] fn protect(_: &[u8],_:bool)->Result<Vec<u8>> { anyhow::bail!("Windows account protection unavailable") }

pub fn load_session()->Result<Option<Session>> {
    if crate::storage::is_demo() { return Ok(None); }
    let path=crate::storage::root().join("cloud-session.dpapi"); crate::storage::safe_chain(&path)?; if !path.exists() { return Ok(None); }
    let meta=fs::symlink_metadata(&path)?; anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=65536,"saved session unsafe");
    let session:Session=serde_json::from_slice(&protect(&fs::read(path)?,true)?)?;
    anyhow::ensure!(valid_server(&session.server) && !session.user_id.is_empty() && session.user_id.len()<=64 &&
        session.user_id.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-') &&
        !session.access_token.is_empty() && session.access_token.len()<=8192 && session.access_token.bytes().all(|b|(0x21..=0x7e).contains(&b)),"saved cloud service invalid"); Ok(Some(session))
}
pub fn login(server:&str,email:&str,password:&str,register:bool,initial_link_consent:bool)->Result<Session> {
    #[cfg(test)]
    if register {return finish_login(server,serde_json::json!({"email":email,"password":password}),"register",initial_link_consent);}
    anyhow::ensure!(!register,"La activación requiere verificar el correo de compra");
    finish_login(server,serde_json::json!({"email":email,"password":password,"device_id":computer_id()?}),"login",initial_link_consent)
}
pub fn start_activation(server:&str,email:&str)->Result<()> {
    start_email_code(server,email,false)
}
pub fn start_email_code(server:&str,email:&str,recover:bool)->Result<()> {
    require_service(server)?;
    let body=serde_json::to_vec(&serde_json::json!({"email":email,"purpose":if recover {"recover"} else {"activate"}}))?;
    crate::auto_update::http_request("POST",&format!("{}/v1/auth/start",normalized_server(server)?),&[("Content-Type".into(),"application/json".into())],&body,65536)?;Ok(())
}
pub fn activate(server:&str,email:&str,code:&str,password:&str,confirmation:&str,consent:bool)->Result<Session> {
    anyhow::ensure!(password==confirmation && (12..=128).contains(&password.chars().count()),"Las contraseñas deben coincidir y tener entre 12 y 128 caracteres");
    finish_login(server,serde_json::json!({"email":email,"code":code,"password":password,"password_confirmation":confirmation,"device_id":computer_id()?}),"activate",consent)
}
pub fn recover_account(server:&str,email:&str,code:&str,password:&str,confirmation:&str,reset_devices:bool,consent:bool)->Result<Session> {
    anyhow::ensure!(password==confirmation && (12..=128).contains(&password.chars().count()),"Las contraseñas deben coincidir y tener entre 12 y 128 caracteres");
    finish_login(server,serde_json::json!({"email":email,"code":code,"password":password,"password_confirmation":confirmation,"device_id":computer_id()?,"reset_devices":reset_devices}),"recover",consent)
}
fn require_service(server:&str)->Result<()> {
    anyhow::ensure!(valid_server(server) && !crate::storage::is_demo(),"Servicio de cuentas no configurado");
    // Customer release builds cannot be redirected to a substitute licence server.
    #[cfg(all(not(debug_assertions),not(test)))]
    anyhow::ensure!(option_env!("WALLET_STUDIO_CLOUD_URL").is_some_and(|endpoint|normalized_server(endpoint).ok()==normalized_server(server).ok()),"El proveedor debe configurar el servicio oficial antes de distribuir esta versión");
    Ok(())
}
pub fn account_app_license(session:&Session)->Result<bool> {
    require_service(&session.server)?;
    let headers=[("Authorization".into(),format!("Bearer {}",session.access_token))];
    let result=crate::auto_update::http_request("GET",&format!("{}/v1/auth/account",session.server),&headers,&[],65536);
    let bytes=match result {
        Err(error) if error.downcast_ref::<crate::auto_update::HttpStatus>().is_some_and(|status|status.0==404)=>
            crate::auto_update::http_request("GET",&format!("{}/v1/auth/session",session.server),&headers,&[],65536)?,
        result=>result?,
    };
    parse_account_license(session,&bytes)
}

fn parse_account_license(session:&Session,bytes:&[u8])->Result<bool> {
    #[derive(Deserialize)] struct Reply {user_id:String,app_licensed:Option<bool>}
    let reply:Reply=serde_json::from_slice(bytes)?;
    anyhow::ensure!(reply.user_id==session.user_id,SessionRejected);
    // Pre-library service sessions were exclusively app licence holders.
    Ok(reply.app_licensed.unwrap_or(true))
}
pub fn validate_session(session:&Session)->Result<()> {
    anyhow::ensure!(account_app_license(session)?,SessionRejected);Ok(())
}

#[derive(Debug)]
struct SessionRejected;
impl std::fmt::Display for SessionRejected {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("La cuenta requiere iniciar sesión de nuevo")}
}
impl std::error::Error for SessionRejected {}
fn session_rejected(error:&anyhow::Error)->bool {
    error.downcast_ref::<SessionRejected>().is_some() || error.downcast_ref::<crate::auto_update::HttpStatus>().is_some_and(|status|matches!(status.0,401|403))
}
// Retaining a previously verified screen on transport failure does not grant
// phone access: require_access still performs an online check for each operation.
pub fn authenticated_after_sync_failure(error:&anyhow::Error,previously_authenticated:bool)->bool {
    !session_rejected(error) && (previously_authenticated || !matches!(error.downcast_ref::<SyncFailure>(),Some(SyncFailure::Session)))
}
pub fn require_access()->Result<()> {
    anyhow::ensure!(!crate::storage::is_demo(),"La demostración no permite operaciones del teléfono");
    let session=load_session()?.context("Inicia sesión con el correo de compra antes de continuar")?;validate_session(&session)
}
#[derive(Clone,Deserialize)] pub struct RegisteredComputer { pub id:String,pub last_seen:u64,#[serde(default)] pub current:bool }
pub fn registered_computers(session:&Session)->Result<Vec<RegisteredComputer>> {
    require_service(&session.server)?;
    let headers=[("Authorization".into(),format!("Bearer {}",session.access_token))];
    let result=crate::auto_update::http_request("GET",&format!("{}/v1/auth/account",session.server),&headers,&[],65536);
    let bytes=match result {Err(error) if error.downcast_ref::<crate::auto_update::HttpStatus>().is_some_and(|status|status.0==404)=>crate::auto_update::http_request("GET",&format!("{}/v1/auth/session",session.server),&headers,&[],65536)?,result=>result?};
    #[derive(Deserialize)] struct Reply {devices:Vec<RegisteredComputer>}
    let mut reply:Reply=serde_json::from_slice(&bytes)?;anyhow::ensure!(reply.devices.len()<=3 && reply.devices.iter().all(|d|d.id.len()==64 && d.id.bytes().all(|b|b.is_ascii_hexdigit())),"Inventario de equipos no válido");
    let current=computer_id()?;for device in &mut reply.devices {device.current=device.id==current;}Ok(reply.devices)
}
pub fn revoke_computer(session:&Session,id:&str)->Result<()> {
    require_service(&session.server)?;
    let body=serde_json::to_vec(&serde_json::json!({"device_id":id}))?;
    crate::auto_update::http_request("POST",&format!("{}/v1/auth/devices/revoke",session.server),&[("Authorization".into(),format!("Bearer {}",session.access_token)),("Content-Type".into(),"application/json".into())],&body,65536)?;Ok(())
}
pub fn forget_session()->Result<()> {
    let path=crate::storage::root().join("cloud-session.dpapi");crate::storage::safe_chain(&path)?;
    if path.exists() {fs::remove_file(path)?;}Ok(())
}
fn computer_id()->Result<String> {
    let root=crate::storage::root();crate::storage::safe_chain(&root)?;fs::create_dir_all(&root)?;
    let _lease=crate::storage::operation_lease_for_import(&root)?;
    let path=root.join("computer-id.dpapi");crate::storage::safe_chain(&path)?;
    if !path.exists() {
        // BCrypt failure is fatal; never silently register an all-zero computer.
        #[link(name="bcrypt")] unsafe extern "system" {fn BCryptGenRandom(algorithm:*mut std::ffi::c_void,buffer:*mut u8,size:u32,flags:u32)->i32;}
        let mut random=[0u8;32];let status=unsafe {BCryptGenRandom(std::ptr::null_mut(),random.as_mut_ptr(),32,2)};
        anyhow::ensure!(status>=0,"No se pudo crear la identidad segura del equipo");
        let id=random.iter().map(|b|format!("{b:02x}")).collect::<String>();
        match crate::storage::write_new(&path,&protect(id.as_bytes(),false)?) {Ok(())=>{},Err(error) if path.exists()=>{let _=error;},Err(error)=>return Err(error)}
    }
    let meta=fs::symlink_metadata(&path)?;anyhow::ensure!(meta.is_file() && meta.len()<=65536,"Identidad de equipo no válida");
    let id=String::from_utf8(protect(&fs::read(path)?,true)?)?;
    anyhow::ensure!(id.len()==64 && id.bytes().all(|b|b.is_ascii_hexdigit()),"Identidad de equipo no válida");Ok(id)
}
fn finish_login(server:&str,body:serde_json::Value,operation:&str,initial_link_consent:bool)->Result<Session> {
    require_service(server)?;
    anyhow::ensure!(!crate::storage::is_demo(),"demo has no account network");
    let server=normalized_server(server)?; let root=crate::storage::root();
    if let Some(owner)=load_owner_at(&root)? { if owner.server!=server { return Err(DifferentVaultOwner.into()); } }
    else { anyhow::ensure!(initial_link_consent,"Confirma la vinculación de todas las copias locales"); }
    let body=serde_json::to_vec(&body)?;
    let bytes=crate::auto_update::http_request("POST",&format!("{server}/v1/auth/{operation}"),&[("Content-Type".into(),"application/json".into())],&body,65536)?;
    #[derive(Deserialize)] struct Reply { access_token:String,user_id:String }
    let reply:Reply=serde_json::from_slice(&bytes)?;
    anyhow::ensure!(!reply.access_token.is_empty() && reply.access_token.len()<=8192 && reply.access_token.bytes().all(|b|(0x21..=0x7e).contains(&b)) &&
        !reply.user_id.is_empty() && reply.user_id.len()<=64 && reply.user_id.bytes().all(|b| b.is_ascii_alphanumeric() || b==b'-'),"invalid account response");
    let session=Session { server,access_token:reply.access_token,user_id:reply.user_id };
    bind_owner_at(&root,&session,initial_link_consent)?;
    let encrypted=protect(&serde_json::to_vec(&session)?,false)?;
    let path=root.join("cloud-session.dpapi"); crate::storage::write_mutable(&path,&encrypted)?; Ok(session)
}
#[derive(Clone,Debug,Default)]
pub struct SyncReport { pub downloaded:usize,pub uploaded:usize,pub verified:usize,pub local_pending:usize,pub pending_reasons:BTreeMap<String,usize> }
impl SyncReport {
    pub fn complete(&self)->bool { self.local_pending==0 }
    pub fn message(&self)->String {
        if self.downloaded+self.uploaded+self.verified+self.local_pending==0 {
            return "Cuenta conectada. No hay copias bancarias verificadas en este perfil ni en la nube. Abre la app con esta cuenta en el PC que conserva los originales para sincronizarlos.".into();
        }
        let summary=format!("Copias: {} descargadas · {} subidas · {} verificadas",self.downloaded,self.uploaded,self.verified);
        if self.local_pending==0 {summary} else {
            let reasons=self.pending_reasons.iter().map(|(reason,count)|format!("{count}: {reason}")).collect::<Vec<_>>().join(" · ");
            format!("{summary}. {} copias locales requieren revisión; se conservan sin subir. {reasons}. No se capturará un original ausente.",self.local_pending)
        }
    }
}
#[derive(Debug)]
enum LocalReview {Busy,Original,Conflict,Recovery,History}
impl std::fmt::Display for LocalReview {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str(match self {
        Self::Busy=>"almacenamiento ocupado o protegido",
        Self::Original=>"original incompleto o integridad no verificable",
        Self::Conflict=>"copia marcada por un conflicto local",
        Self::Recovery=>"recuperación del teléfono pendiente de revisión",
        Self::History=>"historial de cambios pendiente de revisión",
    })}
}
impl std::error::Error for LocalReview {}
fn local_review_message(error:&anyhow::Error)->String {
    error.downcast_ref::<LocalReview>().map(ToString::to_string).unwrap_or_else(||"archivo no exportable con las comprobaciones actuales".into())
}
#[derive(Debug)]
enum SyncFailure {Session,Storage,Inventory,Download,Import,Export,Upload,Receipt}
impl std::fmt::Display for SyncFailure {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { f.write_str(match self {
        Self::Session=>"No se pudo contactar con el servicio para verificar la sesión. Reintenta la conexión; la sesión guardada se conserva.",
        Self::Storage=>"El archivo local necesita revisión o está en uso por otra instancia. Las copias se conservan.",
        Self::Inventory=>"No se pudo consultar el inventario de la nube. Pulsa Sincronizar ahora para reintentar.",
        Self::Download=>"No se pudo descargar y verificar una copia de la nube. Las copias locales se conservan.",
        Self::Import=>"Una copia descargada requiere revisión. No se sobrescribió el original local.",
        Self::Export=>"Hay copias locales que requieren revisión antes de sincronizar.",
        Self::Upload=>"La nube no confirmó la subida. Comprueba la conexión o la capacidad del servicio y reintenta.",
        Self::Receipt=>"No se pudo verificar el resultado de la sincronización. No se confirmó la copia.",
    }) }
}
impl std::error::Error for SyncFailure {}
pub fn sync_failure_message(error:&anyhow::Error)->String {
    if session_rejected(error) {return "El servidor rechazó la sesión. Inicia sesión de nuevo con tu correo y contraseña; tus copias se conservan.".into();}
    let mut message=error.downcast_ref::<SyncFailure>().map(ToString::to_string)
        .unwrap_or_else(||"Sincronización incompleta. Tus copias se conservan; comprueba conexión y almacenamiento.".into());
    if let Some(status)=error.downcast_ref::<crate::auto_update::HttpStatus>() {message.push_str(&format!(" (HTTP {})",status.0));}
    message
}
pub fn synchronize(session:&Session)->Result<()> {
    let report=synchronize_report(session)?;
    anyhow::ensure!(report.complete(),SyncFailure::Export);Ok(())
}
pub fn synchronize_report(session:&Session)->Result<SyncReport> {
    synchronize_report_progress(session,|_|{})
}
pub fn synchronize_report_progress(session:&Session,mut progress:impl FnMut(&'static str))->Result<SyncReport> {
    let mut report=SyncReport::default();
    progress("Conectando con el servicio y verificando la sesión…");
    validate_session(session).context(SyncFailure::Session)?;
    anyhow::ensure!(!crate::storage::is_demo() && valid_server(&session.server),"cloud service unavailable"); crate::storage::ensure_ready().context(SyncFailure::Storage)?;
    let root=crate::storage::root(); let owner=require_owner_at(&root,session)?;
    let headers=vec![("Authorization".into(),format!("Bearer {}",session.access_token))];
    progress("Consultando las copias de tu cuenta…");
    let bytes=crate::auto_update::http_request("GET",&format!("{}/v1/bundles",session.server),&headers,&[],1024*1024).context(SyncFailure::Inventory)?;
    #[derive(Deserialize)] struct Entry { id:String,sha256:String,size:usize }
    #[derive(Deserialize)] struct Listing { bundles:Vec<Entry> }
    let listing:Listing=serde_json::from_slice(&bytes)?; anyhow::ensure!(listing.bundles.len()<=1000,"cloud inventory too large");
    let mut remote_ids=std::collections::BTreeSet::new();
    let seen=seen_directory(&root,&owner)?; crate::storage::safe_chain(&seen)?; fs::create_dir_all(&seen)?;
    for entry in listing.bundles {
        progress("Descargando y verificando originales de la nube…");
        remote_ids.insert(entry.id.clone());
        anyhow::ensure!(entry.id.len()==64 && entry.id.bytes().all(|b|b.is_ascii_hexdigit()) && entry.id==entry.sha256 && entry.size<=LIMIT,"invalid cloud inventory");
        if verified_seen(&root,&seen.join(&entry.id),&entry.id)? {report.verified+=1;continue;}
        let bundle=crate::auto_update::http_request("GET",&format!("{}/v1/bundles/{}",session.server,entry.id),&headers,&[],LIMIT).context(SyncFailure::Download)?;
        anyhow::ensure!(bundle.len()==entry.size && crate::bank_backup::sha256(&bundle)?==entry.sha256,"cloud download checksum mismatch");
        import_bundle(&root,&bundle).context(SyncFailure::Import)?;let (manifest,_)=parse_bundle(&bundle)?;write_seen(&root,&seen.join(&entry.id),&entry.id,manifest).context(SyncFailure::Receipt)?;report.downloaded+=1;
    }
    progress("Verificando y subiendo originales locales…");
    let banks=root.join("bank-backups"); if banks.is_dir() { for entry in fs::read_dir(banks)? {
        // A recovery or incomplete original for one card must not prevent other
        // verified originals being uploaded. Never upload or alter the failed one.
        let path=entry?.path();
        let bundle=match export_bundle(&root,&path) {Ok(bundle)=>bundle,Err(error)=>{
            report.local_pending+=1;*report.pending_reasons.entry(local_review_message(&error)).or_default()+=1;continue;
        }};
        let id=crate::bank_backup::sha256(&bundle)?;
        if remote_ids.contains(&id) && verified_seen(&root,&seen.join(&id),&id)? { continue; }
        let mut upload_headers=headers.clone(); upload_headers.push(("Content-Type".into(),"application/zip".into())); upload_headers.push(("X-Content-SHA256".into(),id.clone()));
        let response=crate::auto_update::http_request("PUT",&format!("{}/v1/bundles/{id}",session.server),&upload_headers,&bundle,65536).context(SyncFailure::Upload)?;
        #[derive(Deserialize)] struct Confirmation {id:String,sha256:String,size:usize}
        let confirmation:Confirmation=serde_json::from_slice(&response).context(SyncFailure::Receipt)?;
        anyhow::ensure!(confirmation.id==id && confirmation.sha256==id && confirmation.size==bundle.len(),SyncFailure::Receipt);
        let (manifest,_)=parse_bundle(&bundle)?;write_seen(&root,&seen.join(&id),&id,manifest).context(SyncFailure::Receipt)?;report.uploaded+=1;
    }} Ok(report)
}

pub fn missing_original_capture_allowed(configured:bool,synced:bool)->bool { !configured || synced }
pub fn account_expected_without_session(endpoint:&str)->bool { account_expected_at(&crate::storage::root(),endpoint) }
fn account_expected_at(root:&Path,endpoint:&str)->bool {
    if valid_server(endpoint) {return true;}
    let path=root.join("vault-owner.json");if crate::storage::safe_chain(&path).is_err() {return true;}
    // Missing/invalid credentials do not turn an already-linked vault into a
    // standalone profile. Even damaged owner evidence requires authentication.
    !matches!(fs::symlink_metadata(path),Err(error) if error.kind()==std::io::ErrorKind::NotFound)
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn transient_sync_failures_do_not_log_out_a_verified_account() {
        let offline=anyhow::anyhow!("synthetic private transport detail").context(SyncFailure::Session);
        assert!(authenticated_after_sync_failure(&offline,true));
        assert!(!authenticated_after_sync_failure(&offline,false));
        assert!(!sync_failure_message(&offline).contains("private transport"));
        let unavailable=anyhow::Error::new(crate::auto_update::HttpStatus(503)).context(SyncFailure::Session);
        assert!(authenticated_after_sync_failure(&unavailable,true));
        assert!(sync_failure_message(&unavailable).contains("HTTP 503"));
        for code in [401,403] {
            for stage in [SyncFailure::Session,SyncFailure::Inventory,SyncFailure::Upload] {
                let rejected=anyhow::Error::new(crate::auto_update::HttpStatus(code)).context(stage);
                assert!(!authenticated_after_sync_failure(&rejected,true));
                assert!(sync_failure_message(&rejected).contains("servidor rechazó"));
            }
        }
        let wrong_identity=anyhow::Error::new(SessionRejected).context(SyncFailure::Session);
        assert!(!authenticated_after_sync_failure(&wrong_identity,true));
        let inventory=anyhow::anyhow!("synthetic inventory failure").context(SyncFailure::Inventory);
        assert!(authenticated_after_sync_failure(&inventory,false),"session check already passed before inventory request");
    }
    #[test] fn customer_missing_original_waits_for_auth_when_service_or_owner_is_known() {
        let root=fixture_root();
        assert!(missing_original_capture_allowed(account_expected_at(&root,""),false));
        assert!(!missing_original_capture_allowed(account_expected_at(&root,"https://sync.example.com"),false));
        crate::storage::write_new(&root.join("vault-owner.json"),b"damaged owner evidence").unwrap();
        assert!(!missing_original_capture_allowed(account_expected_at(&root,""),false));
        assert!(missing_original_capture_allowed(account_expected_at(&root,""),true));fs::remove_dir_all(root).unwrap();
    }
    fn fixture_root()->std::path::PathBuf { std::env::temp_dir().join(format!("ws-cloud-fixture-{}",crate::flasher::generate_token())) }
    fn fixture_baseline()->crate::bank_backup::Baseline {
        let mut png=Vec::new(); image::DynamicImage::new_rgb8(4,4).write_to(&mut Cursor::new(&mut png),image::ImageFormat::Png).unwrap();
        let sha1=crate::bank_backup::sha1(&png).unwrap(); let leaf="cardBackgroundCombined@2x.png";
        crate::bank_backup::Baseline { udid:"synthetic-device-A".into(),card_hash:"OM6NYhwXMZrAw0sRUjR62wmF4ZQ=".into(),
            manifest_bytes:serde_json::to_vec(&serde_json::json!({leaf:{"url":"https://a.apple.com/fixture","size":png.len(),"sha1":sha1}})).unwrap(),
            artwork_name:leaf.into(),remote_size:png.len(),remote_sha1:sha1,remote_artwork_bytes:png.clone(),
            local_assets:crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|name|(name.to_string(),if *name==leaf {crate::bank_backup::LocalAssetState::Present {bytes:png.clone()}} else {crate::bank_backup::LocalAssetState::Absent})).collect() }
    }
    fn repack(manifest:Manifest,files:BTreeMap<String,Vec<u8>>)->Vec<u8> {
        let mut writer=zip::ZipWriter::new(Cursor::new(Vec::new())); let options=zip::write::SimpleFileOptions::default();
        writer.start_file("bundle.json",options).unwrap(); writer.write_all(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        for (name,data) in files {writer.start_file(name,options).unwrap();writer.write_all(&data).unwrap();} writer.finish().unwrap().into_inner()
    }
    #[test] fn customer_owner_binding_requires_consent_and_denies_account_or_server_switch() {
        let root=fixture_root(); let a=Session {server:"https://EXAMPLE.com:443/".into(),user_id:"account-a".into(),access_token:"fixture-token".into()};
        assert!(bind_owner_at(&root,&a,false).is_err()); bind_owner_at(&root,&a,true).unwrap();
        let owner=fs::read(root.join("vault-owner.json")).unwrap();
        let b=Session {user_id:"account-b".into(),..a.clone()}; assert!(bind_owner_at(&root,&b,true).is_err()); assert!(require_owner_at(&root,&b).is_err());
        let c=Session {server:"https://other.example.com".into(),..a.clone()}; assert!(bind_owner_at(&root,&c,true).is_err());
        assert_ne!(seen_directory(&root,&owner_for(&a).unwrap()).unwrap(),seen_directory(&root,&owner_for(&c).unwrap()).unwrap());
        assert_eq!(fs::read(root.join("vault-owner.json")).unwrap(),owner); fs::remove_dir_all(root).unwrap();
    }
    #[test] fn customer_cloud_roundtrip_is_immutable_and_conflict_preserves_both() {
        let source=fixture_root(); let destination=fixture_root(); let input=fixture_baseline();
        let path=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&input).unwrap();
        let bytes=export_bundle(&source,&path).unwrap(); assert_eq!(bytes,export_bundle(&source,&path).unwrap());
        import_bundle(&destination,&bytes).unwrap(); import_bundle(&destination,&bytes).unwrap();
        let imported=destination.join("bank-backups").join(path.file_name().unwrap()); assert_eq!(crate::bank_backup::validate_bundle_directory(&imported).unwrap(),input);
        fs::write(imported.join("manifest.urls"),b"local evidence retained").unwrap(); assert!(import_bundle(&destination,&bytes).is_err());
        assert_eq!(fs::read(imported.join("manifest.urls")).unwrap(),b"local evidence retained"); assert!(!destination.join("storage-blocked.json").exists());
        assert!(crate::storage::ensure_card_ready_at(&destination,&input.udid,&input.card_hash).is_err());
        assert!(crate::storage::ensure_card_ready_at(&destination,"other-synthetic-device",&input.card_hash).is_ok());
        assert!(destination.join("quarantine").join(format!("{}.zip",crate::bank_backup::sha256(&bytes).unwrap())).is_file());
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(destination).unwrap();
    }
    #[test] fn customer_cloud_rejects_body_hash_unknown_slots_and_cross_identity() {
        let source=fixture_root(); let input=fixture_baseline(); let path=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&input).unwrap();
        let bytes=export_bundle(&source,&path).unwrap(); let (manifest,mut files)=parse_bundle(&bytes).unwrap();
        files.values_mut().next().unwrap().push(1); assert!(parse_bundle(&repack(manifest,files)).is_err());
        let (mut manifest,mut files)=parse_bundle(&bytes).unwrap(); let name="bank/identity/credentials.json".to_string(); let data=b"fixture".to_vec();
        manifest.files.insert(name.clone(),Record {size:data.len(),sha256:crate::bank_backup::sha256(&data).unwrap()}); files.insert(name,data);
        assert!(parse_bundle(&repack(manifest,files)).is_err());
        let (mut manifest,files)=parse_bundle(&bytes).unwrap(); manifest.udid="synthetic-device-other".into(); let target=fixture_root();
        assert!(import_bundle(&target,&repack(manifest,files)).is_err()); assert!(!target.join("bank-backups").exists());
        fs::remove_dir_all(source).unwrap(); fs::remove_dir_all(target).unwrap();
    }
    #[test] fn customer_cloud_applied_and_restored_snapshots_merge_in_either_order() {
        let source=fixture_root();let input=fixture_baseline();let path=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&input).unwrap();
        let png=input.remote_artwork_bytes.clone();let pairs=crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|name|(name.to_string(),if name.ends_with(".pdf") {b"%PDF-1.7\n%%EOF".to_vec()} else {png.clone()})).collect::<Vec<_>>();
        let operation=crate::bank_changes::save_fixture_applied_at(&source.join("bank-change-intents"),&input.udid,&input.card_hash,&pairs).unwrap();
        let applied=export_bundle(&source,&path).unwrap();crate::bank_changes::restore_fixture_at(&operation).unwrap();let restored=export_bundle(&source,&path).unwrap();assert_ne!(applied,restored);
        for snapshots in [[&applied,&restored],[&restored,&applied]] {
            let target=fixture_root();for snapshot in snapshots {import_bundle(&target,snapshot).unwrap();}
            let directory=target.join("bank-change-intents").join(operation.file_name().unwrap());assert!(directory.join("state-0004-restored.json").is_file());
            assert_eq!(fs::read(directory.join("state-0002-applied.json")).unwrap(),fs::read(operation.join("state-0002-applied.json")).unwrap());
            crate::bank_changes::validate_cloud_chain(&target.join("bank-change-intents"),&input.udid,&input.card_hash).unwrap();
            assert!(crate::storage::ensure_card_ready_at(&target,&input.udid,&input.card_hash).is_ok());fs::remove_dir_all(target).unwrap();
        }fs::remove_dir_all(source).unwrap();
    }
    #[test] fn customer_cloud_receipts_verify_current_files_and_upgrade_legacy_markers() {
        let source=fixture_root();let target=fixture_root();let input=fixture_baseline();
        let source_path=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&input).unwrap();let bundle=export_bundle(&source,&source_path).unwrap();
        let id=crate::bank_backup::sha256(&bundle).unwrap();import_bundle(&target,&bundle).unwrap();let receipt=target.join("cloud-seen/fixture").join(&id);
        crate::storage::write_new(&receipt,b"verified").unwrap();assert!(!verified_seen(&target,&receipt,&id).unwrap());
        write_seen(&target,&receipt,&id,parse_bundle(&bundle).unwrap().0).unwrap();assert!(verified_seen(&target,&receipt,&id).unwrap());
        let imported=target.join("bank-backups").join(source_path.file_name().unwrap());fs::remove_file(imported.join("remote-artwork.bin")).unwrap();
        assert!(!verified_seen(&target,&receipt,&id).unwrap());import_bundle(&target,&bundle).unwrap();assert!(verified_seen(&target,&receipt,&id).unwrap());
        fs::write(imported.join("remote-artwork.bin"),b"tampered fixture retained").unwrap();assert!(!verified_seen(&target,&receipt,&id).unwrap());
        assert!(import_bundle(&target,&bundle).is_err());assert_eq!(fs::read(imported.join("remote-artwork.bin")).unwrap(),b"tampered fixture retained");
        assert!(!verified_seen(&target,&receipt,&id).unwrap());assert!(crate::storage::ensure_card_ready_at(&target,&input.udid,&input.card_hash).is_err());
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(target).unwrap();
    }
    #[test] fn customer_cloud_older_receipt_allows_new_revisions_but_not_missing_history() {
        let source=fixture_root();let target=fixture_root();let input=fixture_baseline();let path=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&input).unwrap();
        let pairs=crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|name|(name.to_string(),if name.ends_with(".pdf") {b"%PDF-1.7\n%%EOF".to_vec()} else {input.remote_artwork_bytes.clone()})).collect::<Vec<_>>();
        let operation=crate::bank_changes::save_fixture_applied_at(&source.join("bank-change-intents"),&input.udid,&input.card_hash,&pairs).unwrap();let applied=export_bundle(&source,&path).unwrap();
        import_bundle(&target,&applied).unwrap();let id=crate::bank_backup::sha256(&applied).unwrap();let receipt=target.join("cloud-seen/fixture").join(&id);
        write_seen(&target,&receipt,&id,parse_bundle(&applied).unwrap().0).unwrap();
        crate::bank_changes::restore_fixture_at(&operation).unwrap();import_bundle(&target,&export_bundle(&source,&path).unwrap()).unwrap();assert!(verified_seen(&target,&receipt,&id).unwrap());
        let historical=target.join("bank-change-intents").join(operation.file_name().unwrap()).join("state-0002-applied.json");fs::remove_file(&historical).unwrap();
        assert!(!verified_seen(&target,&receipt,&id).unwrap());import_bundle(&target,&applied).unwrap();assert!(historical.is_file());assert!(verified_seen(&target,&receipt,&id).unwrap());
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(target).unwrap();
    }
    #[test] fn customer_cloud_expansion_and_entry_count_are_bounded_before_import() {
        let mut writer=zip::ZipWriter::new(Cursor::new(Vec::new()));let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("bundle.json",options).unwrap();writer.write_all(b"{}").unwrap();writer.start_file("bank/fixture/remote-artwork.bin",options).unwrap();
        let chunk=[0u8;8192];for _ in 0..4097 {writer.write_all(&chunk).unwrap();}
        assert!(parse_bundle(&writer.finish().unwrap().into_inner()).is_err());
        let mut writer=zip::ZipWriter::new(Cursor::new(Vec::new()));let options=zip::write::SimpleFileOptions::default();
        for index in 0..MAX_FILES+2 {writer.start_file(format!("entry-{index}"),options).unwrap();}
        assert!(parse_bundle(&writer.finish().unwrap().into_inner()).is_err());
    }
    /// Only run against the lead's disposable localhost server, in a separate single-threaded test process.
    #[test] #[ignore="requires explicit localhost synthetic backend fixture"]
    fn customer_cloud_server_e2e() {
        assert_eq!(std::env::var("WALLET_STUDIO_TEST_FIXTURE").as_deref(),Ok("1"));
        let server=std::env::var("WALLET_STUDIO_INTEGRATION_URL").expect("fixture server required");
        assert!(server.starts_with("http://127.0.0.1:") && valid_server(&server));
        assert!(crate::auto_update::http_request("GET",&format!("{server}/__fixture_redirect"),&[("Authorization".into(),"Bearer synthetic-redirect-probe".into())],&[],65536).is_err(),"redirects must not forward account headers to the health endpoint");
        let base=fixture_root(); let a=base.join("a");let b=base.join("b");let c=base.join("c");
        fn set_root(path:&Path) { unsafe {std::env::set_var("WALLET_STUDIO_TEST_STORAGE_ROOT",path);} }
        set_root(&a);let computer_a=computer_id().unwrap();assert_eq!(computer_a.len(),64);assert_eq!(computer_id().unwrap(),computer_a);assert_ne!(computer_a,"0".repeat(64));
        assert_ne!(fs::read(a.join("computer-id.dpapi")).unwrap(),computer_a.as_bytes());
        let input=fixture_baseline(); let path=crate::bank_backup::save_fixture_at(&a.join("bank-backups"),&input).unwrap();
        let suffix=crate::flasher::generate_token(); let email_a=format!("fixture-a-{suffix}@example.invalid");let email_b=format!("fixture-b-{suffix}@example.invalid");let password="Synthetic-Fixture-Password-2026";
        let session_a=login(&server,&email_a,password,true,true).unwrap();
        let mut rejected=session_a.clone();rejected.access_token="0".repeat(64);
        let error=validate_session(&rejected).unwrap_err();
        assert!(!authenticated_after_sync_failure(&error,true));
        assert_eq!(error.downcast_ref::<crate::auto_update::HttpStatus>().unwrap().0,401);
        let incomplete=a.join("bank-backups").join("incomplete-fixture");
        fs::create_dir_all(&incomplete).unwrap();fs::write(incomplete.join("preserve.txt"),b"unverified local evidence").unwrap();
        let mut stages=Vec::new();
        let partial=synchronize_report_progress(&session_a,|stage|stages.push(stage)).unwrap();
        assert!(stages[0].contains("sesión"));assert!(stages.iter().any(|stage|stage.contains("locales")));
        assert_eq!(partial.uploaded,1);assert_eq!(partial.local_pending,1);assert!(!partial.complete());
        assert_eq!(partial.pending_reasons.get("original incompleto o integridad no verificable"),Some(&1));
        assert_eq!(fs::read(incomplete.join("preserve.txt")).unwrap(),b"unverified local evidence");
        fs::remove_file(incomplete.join("preserve.txt")).unwrap();fs::remove_dir(incomplete).unwrap();
        synchronize(&session_a).unwrap();
        set_root(&b);assert_ne!(computer_id().unwrap(),computer_a);let session_b=login(&server,&email_a,password,false,true).unwrap();synchronize(&session_b).unwrap();
        let imported=b.join("bank-backups").join(path.file_name().unwrap());assert_eq!(crate::bank_backup::validate_bundle_directory(&imported).unwrap(),input);
        let original_bytes=fs::read(imported.join("baseline.json")).unwrap();synchronize(&session_b).unwrap();assert_eq!(fs::read(imported.join("baseline.json")).unwrap(),original_bytes);
        fs::remove_file(imported.join("remote-artwork.bin")).unwrap();synchronize(&session_b).unwrap();
        assert_eq!(crate::bank_backup::validate_bundle_directory(&imported).unwrap(),input);
        fs::write(imported.join("remote-artwork.bin"),b"tampered existing fixture").unwrap();assert!(synchronize(&session_b).is_err());
        assert_eq!(fs::read(imported.join("remote-artwork.bin")).unwrap(),b"tampered existing fixture");assert!(crate::storage::ensure_card_ready_at(&b,&input.udid,&input.card_hash).is_err());
        set_root(&c);let other=login(&server,&email_b,password,true,true).unwrap();synchronize(&other).unwrap();assert!(!c.join("bank-backups").exists());
        set_root(&a); let previous=fs::read(a.join("cloud-session.dpapi")).unwrap();assert!(login(&server,&email_b,password,false,true).is_err());
        assert_eq!(fs::read(a.join("cloud-session.dpapi")).unwrap(),previous);assert_eq!(load_session().unwrap().unwrap().user_id,session_a.user_id);
        unsafe {std::env::remove_var("WALLET_STUDIO_TEST_STORAGE_ROOT");} assert!(base.is_absolute() && base.starts_with(std::env::temp_dir())); fs::remove_dir_all(base).unwrap();
    }
    #[test] fn service_and_offline_capture_rules_are_conservative() {
        assert!(valid_server("https://sync.example.com")); assert!(valid_server("http://127.0.0.1:8080"));
        assert!(!valid_server("http://example.com")); assert!(!valid_server("https://user@host"));
        assert!(!missing_original_capture_allowed(true,false)); assert!(missing_original_capture_allowed(true,true)); assert!(missing_original_capture_allowed(false,false));
    }
    #[test] fn malformed_and_traversal_archives_are_rejected_before_import() {
        assert!(parse_bundle(b"not a zip").is_err()); assert!(!safe_name("bank/../baseline.json")); assert!(!safe_name("C:/x")); assert!(!safe_name("bank//x"));
    }
}

#[cfg(test)] mod design_license_tests {
    use super::*;
    #[test] fn pack_only_session_never_grants_phone_or_backup_licence_and_legacy_sessions_remain_compatible() {
        let session=Session {server:"https://example.test".into(),access_token:"unused".into(),user_id:"owner".into()};
        assert!(!parse_account_license(&session,br#"{"user_id":"owner","app_licensed":false}"#).unwrap());
        assert!(parse_account_license(&session,br#"{"user_id":"owner","app_licensed":true}"#).unwrap());
        assert!(parse_account_license(&session,br#"{"user_id":"owner"}"#).unwrap());
        assert!(parse_account_license(&session,br#"{"user_id":"other","app_licensed":true}"#).is_err());
    }
}
