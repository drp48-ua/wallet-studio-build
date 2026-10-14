//! Account-scoped immutable original bundles. No network until an account is linked.
use anyhow::{Context,Result};
use serde::{Deserialize,Serialize};
use std::{collections::BTreeMap,fs,io::{Cursor,Read,Write},path::{Path,PathBuf}};
const LIMIT:usize=100*1024*1024;
const MAX_FILES:usize=512;
#[derive(Clone,Serialize,Deserialize)] pub struct Session { pub server:String, pub access_token:String, pub user_id:String }
#[derive(Serialize,Deserialize)] struct Record { size:usize,sha256:String }
#[derive(Serialize,Deserialize)] struct Manifest { schema:u32,udid:String,card_hash:String,files:BTreeMap<String,Record> }

pub fn valid_server(url:&str)->bool {
    !url.contains(['@','?','#','\\']) && !url.chars().any(char::is_whitespace) &&
        ((url.starts_with("https://") && url.split('/').nth(2).is_some_and(|host| !host.is_empty())) || url=="http://127.0.0.1" ||
            url.strip_prefix("http://127.0.0.1:").is_some_and(|port|port.parse::<u16>().is_ok_and(|n|n>0)))
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
        anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=32*1024*1024,"unsafe cloud evidence file");
        let name=format!("{prefix}/{}",path.file_name().context("missing leaf")?.to_string_lossy());
        anyhow::ensure!(safe_name(&name) && evidence_name(&name) && files.len()<MAX_FILES,"cloud evidence name/count invalid");
        files.insert(name,fs::read(path)?);
    } Ok(())
}

pub fn export_bundle(root:&Path,baseline_directory:&Path)->Result<Vec<u8>> {
    let baseline=crate::bank_backup::validate_bundle_directory(baseline_directory)?;
    let pending=crate::wallet_export::pending_recovery_operations()?;
    anyhow::ensure!(!crate::wallet_export::pending_recovery_blocks_device(Ok(&pending),Some(&baseline.udid)),"pending recovery must remain local before sync");
    let key=baseline_directory.file_name().context("missing identity key")?.to_string_lossy();
    let mut files=BTreeMap::new(); files_in(baseline_directory,&format!("bank/{key}"),&mut files)?;
    for operation in crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"),&baseline.udid,&baseline.card_hash)? {
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
    let digest=crate::bank_backup::sha256(bytes)?;
    let (manifest,files)=parse_bundle(bytes)?;
    crate::storage::safe_chain(root)?; fs::create_dir_all(root)?;
    let lock=root.join(".cloud-import.lock");
    let handle=fs::OpenOptions::new().write(true).create_new(true).open(&lock).context("cloud import needs recovery or another process owns it")?;
    let stage=root.join("cloud-staging").join(&digest); fs::create_dir_all(&stage)?;
    let result=(|| {
        let mut bank_key=None;
        for (name,data) in &files {
            let parts:Vec<_>=name.split('/').collect(); anyhow::ensure!(parts.len()==3,"cloud path depth invalid");
            if parts[0]=="bank" { if let Some(key)=bank_key.as_ref() { anyhow::ensure!(key==parts[1],"mixed baseline identities"); } else { bank_key=Some(parts[1].to_string()); } }
            let staged=stage.join(name); if staged.exists() { anyhow::ensure!(fs::read(&staged)?==*data,"staging evidence conflict"); } else { crate::storage::write_new(&staged,data)?; }
        }
        let key=bank_key.context("cloud original missing")?;
        let baseline=crate::bank_backup::validate_bundle_directory(&stage.join("bank").join(&key))?;
        anyhow::ensure!(baseline.udid==manifest.udid && baseline.card_hash==manifest.card_hash,"cloud baseline identity mismatch");
        let staged_intents=stage.join("intents"); fs::create_dir_all(&staged_intents)?;
        let mut merged_bytes=files.values().map(Vec::len).sum::<usize>(); let mut merged_files=files.len();
        for existing in crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"),&manifest.udid,&manifest.card_hash)? {
            let name=existing.file_name().unwrap().to_string_lossy(); let mut existing_files=BTreeMap::new(); files_in(&existing,&format!("intents/{name}"),&mut existing_files)?;
            for (relative,data) in existing_files { let target=stage.join(relative);
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
    else { let quarantine=root.join("quarantine").join(format!("{digest}.zip")); if !quarantine.exists() { crate::storage::write_new(&quarantine,bytes)?; } crate::storage::block(root)?; }
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
    let path=crate::storage::root().join("cloud-session.dpapi"); if !path.exists() { return Ok(None); }
    let meta=fs::symlink_metadata(&path)?; anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=65536,"saved session unsafe");
    let session:Session=serde_json::from_slice(&protect(&fs::read(path)?,true)?)?;
    anyhow::ensure!(valid_server(&session.server) && !session.user_id.is_empty() && session.user_id.len()<=64 &&
        session.user_id.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-') &&
        !session.access_token.is_empty() && session.access_token.len()<=8192 && session.access_token.bytes().all(|b|(0x21..=0x7e).contains(&b)),"saved cloud service invalid"); Ok(Some(session))
}
pub fn login(server:&str,email:&str,password:&str,register:bool)->Result<Session> {
    anyhow::ensure!(valid_server(server) && email.len()<=254 && password.len()<=1024,"account inputs invalid");
    anyhow::ensure!(!crate::storage::is_demo(),"demo has no account network");
    let body=serde_json::to_vec(&serde_json::json!({"email":email,"password":password}))?;
    let bytes=crate::auto_update::http_request("POST",&format!("{}/v1/auth/{}",server.trim_end_matches('/'),if register {"register"} else {"login"}),&[("Content-Type".into(),"application/json".into())],&body,65536)?;
    #[derive(Deserialize)] struct Reply { access_token:String,user_id:String }
    let reply:Reply=serde_json::from_slice(&bytes)?;
    anyhow::ensure!(!reply.access_token.is_empty() && reply.access_token.len()<=8192 && reply.access_token.bytes().all(|b|(0x21..=0x7e).contains(&b)) &&
        !reply.user_id.is_empty() && reply.user_id.len()<=64 && reply.user_id.bytes().all(|b| b.is_ascii_alphanumeric() || b==b'-'),"invalid account response");
    let session=Session { server:server.trim_end_matches('/').into(),access_token:reply.access_token,user_id:reply.user_id };
    let encrypted=protect(&serde_json::to_vec(&session)?,false)?;
    let root=crate::storage::root(); fs::create_dir_all(&root)?; let path=root.join("cloud-session.dpapi"); let temporary=root.join(format!(".session-{}",crate::flasher::generate_token()));
    crate::storage::write_new(&temporary,&encrypted)?; if path.exists() { fs::remove_file(&path)?; } fs::rename(temporary,path)?; Ok(session)
}
pub fn synchronize(session:&Session)->Result<()> {
    anyhow::ensure!(!crate::storage::is_demo() && valid_server(&session.server),"cloud service unavailable"); crate::storage::ensure_ready()?;
    let headers=vec![("Authorization".into(),format!("Bearer {}",session.access_token))];
    let bytes=crate::auto_update::http_request("GET",&format!("{}/v1/bundles",session.server),&headers,&[],1024*1024)?;
    #[derive(Deserialize)] struct Entry { id:String,sha256:String,size:usize }
    #[derive(Deserialize)] struct Listing { bundles:Vec<Entry> }
    let listing:Listing=serde_json::from_slice(&bytes)?; anyhow::ensure!(listing.bundles.len()<=1000,"cloud inventory too large");
    let root=crate::storage::root(); let seen=root.join("cloud-seen").join(&session.user_id); crate::storage::safe_chain(&seen)?; fs::create_dir_all(&seen)?;
    for entry in listing.bundles {
        anyhow::ensure!(entry.id.len()==64 && entry.id.bytes().all(|b|b.is_ascii_hexdigit()) && entry.id==entry.sha256 && entry.size<=LIMIT,"invalid cloud inventory");
        if seen.join(&entry.id).exists() { continue; }
        let bundle=crate::auto_update::http_request("GET",&format!("{}/v1/bundles/{}",session.server,entry.id),&headers,&[],LIMIT)?;
        anyhow::ensure!(bundle.len()==entry.size && crate::bank_backup::sha256(&bundle)?==entry.sha256,"cloud download checksum mismatch");
        import_bundle(&root,&bundle)?; crate::storage::write_new(&seen.join(&entry.id),b"verified")?;
    }
    let banks=root.join("bank-backups"); if banks.is_dir() { for entry in fs::read_dir(banks)? {
        let bundle=export_bundle(&root,&entry?.path())?; let id=crate::bank_backup::sha256(&bundle)?;
        if seen.join(&id).exists() { continue; }
        let mut upload_headers=headers.clone(); upload_headers.push(("Content-Type".into(),"application/zip".into())); upload_headers.push(("X-Content-SHA256".into(),id.clone()));
        crate::auto_update::http_request("PUT",&format!("{}/v1/bundles/{id}",session.server),&upload_headers,&bundle,65536)?;
        crate::storage::write_new(&seen.join(id),b"verified")?;
    }} Ok(())
}

pub fn missing_original_capture_allowed(configured:bool,_synced:bool)->bool { !configured }
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn service_and_offline_capture_rules_are_conservative() {
        assert!(valid_server("https://sync.example.com")); assert!(valid_server("http://127.0.0.1:8080"));
        assert!(!valid_server("http://example.com")); assert!(!valid_server("https://user@host"));
        assert!(!missing_original_capture_allowed(true,false)); assert!(!missing_original_capture_allowed(true,true)); assert!(missing_original_capture_allowed(false,false));
    }
    #[test] fn malformed_and_traversal_archives_are_rejected_before_import() {
        assert!(parse_bundle(b"not a zip").is_err()); assert!(!safe_name("bank/../baseline.json")); assert!(!safe_name("C:/x")); assert!(!safe_name("bank//x"));
    }
}
