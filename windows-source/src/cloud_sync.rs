//! Account-scoped immutable original bundles. No network until an account is linked.
use anyhow::{Context,Result};
use serde::{Deserialize,Serialize};
use std::{collections::BTreeMap,fs,io::{Cursor,Read,Write},path::Path};
const LIMIT:usize=100*1024*1024;
const MAX_FILES:usize=512;
// Match the existing service object cap; never increase it to fit local history.
const UPLOAD_LIMIT:usize=32*1024*1024;
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

fn export_files(root:&Path,baseline_directory:&Path)->Result<(crate::bank_backup::Baseline,BTreeMap<String,Vec<u8>>)> {
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
    Ok((baseline,files))
}
pub fn export_bundle(root:&Path,baseline_directory:&Path)->Result<Vec<u8>> {
    let _lease=crate::storage::operation_lease_for_import(root).context(LocalReview::Busy)?;
    let (baseline,files)=export_files(root,baseline_directory)?;
    encode_bundle(&baseline,&files,zip::CompressionMethod::Stored)
}
fn encode_bundle(baseline:&crate::bank_backup::Baseline,files:&BTreeMap<String,Vec<u8>>,compression:zip::CompressionMethod)->Result<Vec<u8>> {
    let records=files.iter().map(|(name,bytes)|Ok((name.clone(),Record { size:bytes.len(),sha256:crate::bank_backup::sha256(bytes)? }))).collect::<Result<_>>()?;
    let manifest=Manifest { schema:1,udid:baseline.udid.clone(),card_hash:baseline.card_hash.clone(),files:records };
    let mut archive=zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options=zip::write::SimpleFileOptions::default().compression_method(compression).unix_permissions(0o600);
    archive.start_file("bundle.json",options)?; archive.write_all(&serde_json::to_vec(&manifest)?)?;
    for (name,bytes) in files { archive.start_file(name,options)?; archive.write_all(&bytes)?; }
    let bytes=archive.finish()?.into_inner(); anyhow::ensure!(bytes.len()<=LIMIT,"cloud archive too large"); Ok(bytes)
}

fn partition_bundles(baseline:&crate::bank_backup::Baseline,files:BTreeMap<String,Vec<u8>>,limit:usize)->Result<Vec<Vec<u8>>> {
    let bank:BTreeMap<_,_>=files.iter().filter(|(name,_)|name.starts_with("bank/")).map(|(name,data)|(name.clone(),data.clone())).collect();
    let mut operations:BTreeMap<String,BTreeMap<String,Vec<u8>>>=BTreeMap::new();
    for (name,data) in files {if name.starts_with("intents/") {
        let operation=name.split('/').nth(1).context("cloud operation missing")?.to_string();
        operations.entry(operation).or_default().insert(name,data);
    }}
    let encode=|files:&BTreeMap<String,Vec<u8>>|encode_bundle(baseline,files,zip::CompressionMethod::Deflated);
    let mut current=bank.clone();let mut current_bytes=encode(&current)?;let mut bundles=Vec::new();
    anyhow::ensure!(current_bytes.len()<=limit,LocalReview::OriginalSize);
    for operation in operations.into_values() {
        let mut candidate=current.clone();candidate.extend(operation.clone());let candidate_bytes=encode(&candidate)?;
        if candidate_bytes.len()<=limit {current=candidate;current_bytes=candidate_bytes;continue;}
        // A complete operation is indivisible: all revisions and payloads stay together.
        let mut single=bank.clone();single.extend(operation);let single_bytes=encode(&single)?;
        anyhow::ensure!(single_bytes.len()<=limit,LocalReview::OperationSize);
        if current.len()>bank.len() {bundles.push(current_bytes);}
        current=single;current_bytes=single_bytes;
    }
    bundles.push(current_bytes);Ok(bundles)
}
fn export_bundles(root:&Path,baseline_directory:&Path,limit:usize)->Result<Vec<Vec<u8>>> {
    let _lease=crate::storage::operation_lease_for_import(root).context(LocalReview::Busy)?;
    let (baseline,files)=export_files(root,baseline_directory)?;
    anyhow::ensure!(limit<=UPLOAD_LIMIT,"upload cap cannot exceed service cap");
    // Preserve existing content-addressed receipts when the complete legacy
    // encoding already fits. Compress/partition only oversized snapshots.
    let legacy=encode_bundle(&baseline,&files,zip::CompressionMethod::Stored)?;
    if legacy.len()<=limit {return Ok(vec![legacy]);}
    partition_bundles(&baseline,files,limit)
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

// Only semantic conflicts for an exact validated identity are eligible for
// per-card isolation. Filesystem, parsing and transport errors remain fatal.
#[derive(Debug)]
struct ImportConflict;
impl std::fmt::Display for ImportConflict {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("conflicting immutable cloud/local evidence")}
}
impl std::error::Error for ImportConflict {}
fn isolated_import_conflict(error:&anyhow::Error)->bool {
    error.downcast_ref::<ImportConflict>().is_some()
        || error.downcast_ref::<crate::bank_changes::PendingCloudHistory>().is_some()
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
        let baseline=match crate::bank_backup::validate_bundle_directory(&stage.join("bank").join(&key)) {
            Ok(baseline)=>baseline,
            Err(error)=>{if let Some(unproven)=error.downcast_ref::<crate::bank_backup::UnprovenOriginal>() {
                let evidence=unproven.evidence();anyhow::ensure!(evidence.udid==manifest.udid && evidence.card_hash==manifest.card_hash,"unproven original identity mismatch");
                verified_identity=Some((evidence.udid.clone(),evidence.card_hash.clone()));
            }return Err(error);}
        };
        anyhow::ensure!(baseline.udid==manifest.udid && baseline.card_hash==manifest.card_hash,"cloud baseline identity mismatch");
        verified_identity=Some((baseline.udid.clone(),baseline.card_hash.clone()));
        // Validate the incoming snapshot alone before a transient overlap with
        // older local snapshots can be classified as an isolated union conflict.
        materialize_inventory_archive(&root.join("cloud-validated").join(&digest),&manifest,&files)?;
        let staged_intents=stage.join("intents"); fs::create_dir_all(&staged_intents)?;
        let mut merged_bytes=files.values().map(Vec::len).sum::<usize>(); let mut merged_files=files.len();
        for existing in crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"),&manifest.udid,&manifest.card_hash)? {
            let name=existing.file_name().unwrap().to_string_lossy(); let mut existing_files=BTreeMap::new(); files_in(&existing,&format!("intents/{name}"),&mut existing_files)?;
            for (relative,data) in existing_files { let target=stage.join(relative);crate::storage::safe_chain(&target)?;
                if target.exists() { anyhow::ensure!(fs::read(&target)?==data,ImportConflict); }
                else { merged_bytes=merged_bytes.checked_add(data.len()).context("merged size overflow")?; merged_files+=1;
                    anyhow::ensure!(merged_bytes<=LIMIT && merged_files<=MAX_FILES,"merged history too large; preserve local evidence");
                    crate::storage::write_new(&target,&data)?; }
            }
        }
        crate::bank_changes::validate_cloud_chain(&staged_intents,&manifest.udid,&manifest.card_hash).map_err(|error| {
            // Only the union of independently valid incoming/local snapshots can
            // be waiting for a later restored revision. Never reclassify errors
            // from validating either snapshot alone, including IO and parsing.
            if error.downcast_ref::<crate::bank_changes::OverlappingCloudHistory>().is_some() {anyhow::Error::from(ImportConflict)} else {error}
        })?;
        for (name,data) in &files {
            let (prefix,tail)=name.split_once('/').unwrap();
            let destination=root.join(if prefix=="bank" {"bank-backups"} else {"bank-change-intents"}).join(tail);
            crate::storage::safe_chain(&destination)?;
            if destination.exists() { anyhow::ensure!(fs::read(&destination)?==*data,ImportConflict); }
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

// Read only bounded, regular, integrity-bound quarantine evidence. Errors must
// propagate: an unreadable archive could belong to the requested identity.
fn read_quarantine_archive(path:&Path)->Result<(Manifest,Vec<u8>)> {
    crate::storage::safe_chain(path)?;
    let metadata=fs::symlink_metadata(path)?;
    anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink()
        && metadata.len()<=LIMIT as u64,"quarantine evidence unsafe or too large");
    let bytes=fs::read(path)?;
    anyhow::ensure!(path.file_stem().and_then(|name|name.to_str())==Some(crate::bank_backup::sha256(&bytes)?.as_str()),
        "quarantine archive integrity mismatch");
    let (manifest,_)=parse_bundle(&bytes)?;
    Ok((manifest,bytes))
}
// Legacy migration retains rejected cards.json as aliases-<digest>.json.
// These bounded names are never original/history/identity authority. Recognize
// only the exact artifact shape and preserve the file verbatim in quarantine.
fn is_retained_alias_artifact(path:&Path)->Result<bool> {
    let Some(digest)=path.file_name().and_then(|name|name.to_str()).and_then(|name|name.strip_prefix("aliases-")).and_then(|name|name.strip_suffix(".json")) else {return Ok(false);};
    if digest.len()!=64 || !digest.bytes().all(|byte|byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {return Ok(false);}
    crate::storage::safe_chain(path)?;let metadata=fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len()>1024*1024 {return Ok(false);}
    let mut bytes=Vec::new();fs::File::open(path)?.take(1024*1024+1).read_to_end(&mut bytes)?;
    if bytes.len()>1024*1024 || bytes.len() as u64!=metadata.len() || crate::bank_backup::sha256(&bytes)?!=digest {return Ok(false);}
    #[derive(Deserialize)] #[serde(deny_unknown_fields)] struct Alias {hash:String,name:String}
    let aliases:Vec<Alias>=match serde_json::from_slice(&bytes) {Ok(aliases)=>aliases,Err(_)=>return Ok(false)};
    Ok(aliases.len()<=10000 && aliases.iter().all(|alias|alias.hash.len()<=1024 && alias.name.len()<=256))
}
pub fn reconcile_card_quarantine<L>(udid: &str, hash: &str, log: &mut L) -> Result<bool>
where L: FnMut(&str) {
    let root = crate::storage::root();
    reconcile_card_quarantine_at(&root, udid, hash, log)
}

pub(crate) fn reconcile_card_quarantine_at<L>(root: &Path, udid: &str, hash: &str, log: &mut L) -> Result<bool>
where L: FnMut(&str) {
    let _lease = crate::storage::operation_lease_for_import(root)?;
    crate::storage::safe_chain(root)?;
    if crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"), udid, hash).is_err() {
        return Ok(false);
    }
    let quarantine_dir = root.join("quarantine");
    // Missing evidence cannot explain why a review marker was created.
    if !quarantine_dir.exists() { return Ok(false); }
    crate::storage::safe_chain(&quarantine_dir)?;
    let mut matching_zips = Vec::new();
    for entry in fs::read_dir(&quarantine_dir)? {
        let path = entry?.path();
        if is_retained_alias_artifact(&path)? {continue;}
        if path.extension().and_then(|s| s.to_str()) != Some("zip") { return Ok(false); }
        // Unknown identity is not evidence of an unrelated card. Validate every
        // archive before any import or deletion, retaining review on failure.
        let (manifest, bytes) = read_quarantine_archive(&path)?;
        if manifest.udid == udid && manifest.card_hash == hash {
            matching_zips.push((path, bytes));
        }
    }
    if matching_zips.is_empty() { return Ok(false); }
    let mut all_imported=true;
    for (path, bytes) in &matching_zips {
        if import_bundle(root, bytes).is_ok() {
            log(&format!("Paquete en cuarentena reconciliado e importado con éxito: {}", path.file_name().unwrap().to_string_lossy()));
        } else {
            all_imported=false;
            log(&format!("El paquete en cuarentena mantiene diferencias con la evidencia local; se conserva: {}", path.file_name().unwrap().to_string_lossy()));
        }
    }
    // Retain even compatible snapshots until the entire identity is resolved:
    // no false receipt and no repeated download while another snapshot blocks it.
    if !all_imported {return Ok(false);}
    for (path,bytes) in &matching_zips {anyhow::ensure!(read_quarantine_archive(path)?.1==*bytes,"quarantine changed before reconciliation");}
    for (path,_) in &matching_zips {fs::remove_file(path)?;}
    let mut still_quarantined = false;
    for entry in fs::read_dir(&quarantine_dir)? {
        let path = entry?.path();
        if is_retained_alias_artifact(&path)? {continue;}
        if path.extension().and_then(|s| s.to_str()) != Some("zip") {return Ok(false);}
        let (manifest, _) = read_quarantine_archive(&path)?;
        if manifest.udid == udid && manifest.card_hash == hash {
            still_quarantined = true;
            break;
        }
    }
    if !still_quarantined && crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"), udid, hash).is_ok() {
        crate::storage::clear_card_review_at(root, udid, hash)?;
        log("Revisión de tarjeta liberada tras reconciliar toda la evidencia cloud.");
        return Ok(true);
    }
    Ok(false)
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
    load_session_at(&crate::storage::root())
}
fn load_session_at(root:&Path)->Result<Option<Session>> {
    let path=root.join("cloud-session.dpapi"); crate::storage::safe_chain(&path)?; if !path.exists() { return Ok(None); }
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
/// A readable DPAPI session can retain the library/account screen while offline.
/// This is not licence verification: pending sessions cannot start new phone work.
pub fn retained_session_after_failure(error:&anyhow::Error)->bool { !session_rejected(error) }
pub fn session_connection_failure_message(error:&anyhow::Error)->String {
    if session_rejected(error) {return sync_failure_message(error);}
    let mut message="Sesión guardada en este equipo. Conexión pendiente; se reintentará automáticamente sin pedir tu contraseña. El iPhone requiere verificar el acceso en línea.".to_owned();
    if let Some(status)=error.downcast_ref::<crate::auto_update::HttpStatus>() {message.push_str(&format!(" (HTTP {})",status.0));}
    message
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
    let _lease=crate::storage::operation_lease_for_import(&crate::storage::root())?;
    let path=crate::storage::root().join("cloud-session.dpapi");crate::storage::safe_chain(&path)?;
    if path.exists() {fs::remove_file(path)?;}Ok(())
}
pub fn forget_rejected_session(rejected:&Session)->Result<()> {
    forget_rejected_session_at(&crate::storage::root(),rejected)
}
fn forget_rejected_session_at(root:&Path,rejected:&Session)->Result<()> {
    let _lease=crate::storage::operation_lease_for_import(root)?;
    // Do not erase a newer login saved by another running app instance.
    if load_session_at(root)?.is_some_and(|current| current.server==rejected.server && current.user_id==rejected.user_id && current.access_token==rejected.access_token) {
        fs::remove_file(root.join("cloud-session.dpapi"))?;
    }
    Ok(())
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
    let _lease=crate::storage::operation_lease_for_import(&root)?;
    bind_owner_at(&root,&session,initial_link_consent)?;
    let encrypted=protect(&serde_json::to_vec(&session)?,false)?;
    let path=root.join("cloud-session.dpapi"); crate::storage::write_mutable(&path,&encrypted)?; Ok(session)
}
#[derive(Clone,Debug,Default)]
pub struct SyncReport { pub downloaded:usize,pub uploaded:usize,pub verified:usize,pub local_pending:usize,pub remote_pending:usize,pub rejected_originals:usize,pub pending_reasons:BTreeMap<String,usize> }
impl SyncReport {
    pub fn complete(&self)->bool { self.local_pending==0 && self.remote_pending==0 }
    pub fn message(&self)->String {
        if self.downloaded+self.uploaded+self.verified+self.local_pending+self.remote_pending+self.rejected_originals==0 {
            return "Cuenta conectada. No hay copias bancarias verificadas en este perfil ni en la nube. Abre la app con esta cuenta en el PC que conserva los originales para sincronizarlos.".into();
        }
        let mut summary=format!("Copias: {} descargadas · {} subidas · {} verificadas",self.downloaded,self.uploaded,self.verified);
        if self.rejected_originals>0 {summary.push_str(&format!(". {} archivos antiguos no se usan como original; se conservan",self.rejected_originals));}
        if self.complete() {return summary;}
        let mut details=Vec::new();
        if self.remote_pending>0 {details.push(format!("{} copias de la nube requieren revisión; el original local y la copia en cuarentena se conservan. Las demás copias se han seguido procesando",self.remote_pending));}
        if self.local_pending>0 {
            let reasons=self.pending_reasons.iter().map(|(reason,count)|format!("{count}: {reason}")).collect::<Vec<_>>().join(" · ");
            details.push(format!("{} copias locales requieren revisión; se conservan sin subir. {reasons}",self.local_pending));
        }
        format!("{summary}. {}. No se capturará un original ausente.",details.join(". "))
    }
}
#[derive(Debug)]
enum LocalReview {Busy,Original,Conflict,Recovery,History,OriginalSize,OperationSize}
impl std::fmt::Display for LocalReview {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str(match self {
        Self::Busy=>"almacenamiento ocupado o protegido",
        Self::Original=>"original incompleto o integridad no verificable",
        Self::Conflict=>"copia marcada por un conflicto local",
        Self::Recovery=>"recuperación del teléfono pendiente de revisión",
        Self::History=>"historial de cambios pendiente de revisión",
        Self::OriginalSize=>"el original íntegro supera 32 MiB; se conserva localmente sin recortar el historial",
        Self::OperationSize=>"una operación íntegra con su original supera 32 MiB; se conserva todo el historial local",
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
    if error.downcast_ref::<crate::bank_changes::PendingCloudHistory>().is_some() {
        return "Un cambio anterior quedó sin terminar. Conecta el iPhone con el que hiciste ese cambio y usa Restaurar original en Tarjetas. Después pulsa Sincronizar ahora. Tus copias se conservan; no es una descarga en curso.".into();
    }
    let mut message=error.downcast_ref::<SyncFailure>().map(ToString::to_string)
        .unwrap_or_else(||"Sincronización incompleta. Tus copias se conservan; comprueba conexión y almacenamiento.".into());
    if let Some(status)=error.downcast_ref::<crate::auto_update::HttpStatus>() {message.push_str(&format!(" (HTTP {})",status.0));}
    if error.downcast_ref::<crate::auto_update::RequestTimeout>().is_some() {
        message.push_str(" La descarga agotó el tiempo de espera. Pulsa Sincronizar ahora para reintentar.");
    }
    message
}
pub fn synchronize(session:&Session)->Result<()> {
    let report=synchronize_report(session)?;
    anyhow::ensure!(report.remote_pending==0,SyncFailure::Import);
    anyhow::ensure!(report.complete(),SyncFailure::Export);Ok(())
}
pub fn synchronize_report(session:&Session)->Result<SyncReport> {
    synchronize_report_progress(session,|_|{})
}
pub fn synchronize_report_progress(session:&Session,mut progress:impl FnMut(&'static str))->Result<SyncReport> {
    progress("Conectando con el servicio y verificando la sesión…");
    validate_session(session).context(SyncFailure::Session)?;
    anyhow::ensure!(!crate::storage::is_demo() && valid_server(&session.server),"cloud service unavailable"); crate::storage::ensure_ready().context(SyncFailure::Storage)?;
    let root=crate::storage::root();
    synchronize_owned_report(&root,session,&mut progress,&mut |method,url,headers,body,max| {
        if method=="GET" && max==LIMIT {
            crate::auto_update::http_download_bundle(url,headers,max)
        } else {crate::auto_update::http_request(method,url,headers,body,max)}
    })
}
// A fresh authenticated inventory still controls eligibility. This cache is
// retained conflict evidence, never an offline ownership grant or seen receipt.
fn retained_inventory_bundle(root:&Path,id:&str,size:usize)->Result<Option<Vec<u8>>> {
    anyhow::ensure!(id.len()==64 && id.bytes().all(|byte|byte.is_ascii_hexdigit()) && size<=LIMIT,"invalid retained inventory entry");
    let quarantine=root.join("quarantine").join(format!("{id}.zip"));
    crate::storage::safe_chain(&quarantine)?;
    let path=if quarantine.exists() {quarantine} else {root.join("rejected-originals").join(format!("{id}.zip"))};
    crate::storage::safe_chain(&path)?;
    let metadata=match fs::symlink_metadata(&path) {
        Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(None),result=>result?
    };
    anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink()
        && metadata.len()==size as u64 && size<=LIMIT,"retained cloud evidence requires review");
    let bytes=fs::read(&path)?;
    anyhow::ensure!(bytes.len()==size && crate::bank_backup::sha256(&bytes)?==id,"retained cloud evidence integrity mismatch");
    Ok(Some(bytes))
}
// Transport injection exercises the production import/upload loop with synthetic
// bytes. Authentication and storage initialization stay in the public entrypoint.
type SyncTransport<'a> = dyn FnMut(&str,&str,&[(String,String)],&[u8],usize)->Result<Vec<u8>> + 'a;
fn synchronize_owned_report(root:&Path,session:&Session,progress:&mut dyn FnMut(&'static str),
    request:&mut SyncTransport<'_>)->Result<SyncReport> {
    synchronize_owned_report_with_limit(root,session,progress,request,UPLOAD_LIMIT)
}
fn synchronize_owned_report_with_limit(root:&Path,session:&Session,progress:&mut dyn FnMut(&'static str),
    request:&mut SyncTransport<'_>,upload_limit:usize)->Result<SyncReport> {
    let mut report=SyncReport::default();
    let owner=require_owner_at(root,session)?;
    let headers=vec![("Authorization".into(),format!("Bearer {}",session.access_token))];
    progress("Consultando las copias de tu cuenta…");
    let bytes=request("GET",&format!("{}/v1/bundles",session.server),&headers,&[],1024*1024).context(SyncFailure::Inventory)?;
    #[derive(Deserialize)] struct Entry { id:String,sha256:String,size:usize }
    #[derive(Deserialize)] struct Listing { bundles:Vec<Entry> }
    let listing:Listing=serde_json::from_slice(&bytes)?; anyhow::ensure!(listing.bundles.len()<=1000,"cloud inventory too large");
    let mut remote_ids=std::collections::BTreeSet::new();
    let mut pending_ids=std::collections::BTreeSet::new();
    let mut legacy_identities=std::collections::BTreeSet::new();
    let mut pending_manifests=BTreeMap::new();
    let seen=seen_directory(&root,&owner)?; crate::storage::safe_chain(&seen)?; fs::create_dir_all(&seen)?;
    for entry in listing.bundles {
        progress("Descargando y verificando originales de la nube…");
        remote_ids.insert(entry.id.clone());
        anyhow::ensure!(entry.id.len()==64 && entry.id.bytes().all(|b|b.is_ascii_hexdigit()) && entry.id==entry.sha256 && entry.size<=LIMIT,"invalid cloud inventory");
        if verified_seen(&root,&seen.join(&entry.id),&entry.id)? {report.verified+=1;continue;}
        let bundle=match retained_inventory_bundle(root,&entry.id,entry.size).context(SyncFailure::Storage)? {
            Some(bundle)=>bundle,
            None=>request("GET",&format!("{}/v1/bundles/{}",session.server,entry.id),&headers,&[],LIMIT).context(SyncFailure::Download)?,
        };
        anyhow::ensure!(bundle.len()==entry.size && crate::bank_backup::sha256(&bundle).context(SyncFailure::Download)?==entry.sha256,SyncFailure::Download);
        if let Err(error)=import_bundle(root,&bundle) {
            if error.downcast_ref::<crate::bank_backup::UnprovenOriginal>().is_some() {
                let evidence=classify_rejected_archive(root,&bundle)?.context("unproven archive failed complete classification")?;
                persist_exact(&root.join("rejected-originals").join(format!("{}.zip",entry.id)),&bundle)?;
                legacy_identities.insert((evidence.udid,evidence.card_hash));pending_ids.insert(entry.id.clone());report.remote_pending+=1;continue;
            }
            if !isolated_import_conflict(&error) {return Err(error.context(SyncFailure::Import));}
            // import_bundle returns the semantic error only after the exact ZIP
            // is retained and the identity is blocked. Never write a seen receipt.
            pending_ids.insert(entry.id.clone());report.remote_pending+=1;
            pending_manifests.insert(entry.id.clone(),parse_bundle(&bundle)?.0);
            continue;
        }
        let (manifest,_)=parse_bundle(&bundle)?;
        if crate::storage::card_is_blocked_at(root,&manifest.udid,&manifest.card_hash) {
            // An older compatible snapshot cannot clear another snapshot's
            // review marker. Retain it, but continue other identities.
            let retained=root.join("quarantine").join(format!("{}.zip",entry.id));
            crate::storage::safe_chain(&retained).context(SyncFailure::Storage)?;
            if retained.exists() {anyhow::ensure!(fs::read(&retained)?==bundle,SyncFailure::Storage);}
            else {crate::storage::write_new(&retained,&bundle).context(SyncFailure::Storage)?;}
            pending_ids.insert(entry.id.clone());report.remote_pending+=1;
            pending_manifests.insert(entry.id.clone(),manifest);
            continue;
        }
        write_seen(&root,&seen.join(&entry.id),&entry.id,manifest).context(SyncFailure::Receipt)?;report.downloaded+=1;
    }
    for (udid,hash) in legacy_identities {
        if let Some((resolved,rejected))=resolve_rejected_originals(root,&owner,&remote_ids,&udid,&hash)? {
            report.rejected_originals+=rejected;report.downloaded+=resolved.len().saturating_sub(rejected);
            for id in resolved {if pending_ids.remove(&id) {report.remote_pending-=1;}}
        }
    }
    // Complete snapshots can arrive in arbitrary inventory order. A newer
    // restored revision may now resolve a previously ambiguous partial union.
    let identities:std::collections::BTreeSet<_>=pending_manifests.values().map(|manifest|(manifest.udid.clone(),manifest.card_hash.clone())).collect();
    let mut reconciled=std::collections::BTreeSet::new();
    for (udid,hash) in identities {
        if reconcile_card_quarantine_at(root,&udid,&hash,&mut |_|{})? {
            reconciled.insert((udid,hash));
        }
    }
    for (id,manifest) in pending_manifests {
        if reconciled.contains(&(manifest.udid.clone(),manifest.card_hash.clone())) && pending_ids.contains(&id) {
            // The exact current inventory archive was positively imported;
            // today's complete local evidence must still match its receipt.
            write_seen(root,&seen.join(&id),&id,manifest)?;
            pending_ids.remove(&id);report.remote_pending-=1;report.downloaded+=1;
        }
    }
    progress("Verificando y subiendo originales locales…");
    let banks=root.join("bank-backups"); if banks.is_dir() { for entry in fs::read_dir(banks)? {
        // A recovery or incomplete original for one card must not prevent other
        // verified originals being uploaded. Never upload or alter the failed one.
        let path=entry?.path();
        let bundles=match export_bundles(&root,&path,upload_limit) {Ok(bundles)=>bundles,Err(error)=>{
            report.local_pending+=1;*report.pending_reasons.entry(local_review_message(&error)).or_default()+=1;continue;
        }};
        for bundle in bundles {
        let id=crate::bank_backup::sha256(&bundle)?;
        if remote_ids.contains(&id) && verified_seen(&root,&seen.join(&id),&id)? { continue; }
        let mut upload_headers=headers.clone(); upload_headers.push(("Content-Type".into(),"application/zip".into())); upload_headers.push(("X-Content-SHA256".into(),id.clone()));
        let response=request("PUT",&format!("{}/v1/bundles/{id}",session.server),&upload_headers,&bundle,65536).context(SyncFailure::Upload)?;
        #[derive(Deserialize)] struct Confirmation {id:String,sha256:String,size:usize}
        let confirmation:Confirmation=serde_json::from_slice(&response).context(SyncFailure::Receipt)?;
        anyhow::ensure!(confirmation.id==id && confirmation.sha256==id && confirmation.size==bundle.len(),SyncFailure::Receipt);
        let (manifest,_)=parse_bundle(&bundle)?;write_seen(&root,&seen.join(&id),&id,manifest).context(SyncFailure::Receipt)?;report.uploaded+=1;
        }
    }} Ok(report)
}

fn same_issuer(a:&crate::bank_backup::Baseline,b:&crate::bank_backup::Baseline)->bool {
    a.udid==b.udid && a.card_hash==b.card_hash && a.manifest_bytes==b.manifest_bytes
        && a.artwork_name==b.artwork_name && a.remote_artwork_bytes==b.remote_artwork_bytes
        && a.remote_size==b.remote_size && a.remote_sha1==b.remote_sha1
}
fn classify_rejected_archive(root:&Path,bytes:&[u8])->Result<Option<crate::bank_backup::Baseline>> {
    let (manifest,files)=parse_bundle(bytes)?;let id=crate::bank_backup::sha256(bytes)?;
    let (evidence,unproven)=materialize_inventory_evidence(&root.join("original-review-evidence").join(id),&manifest,&files)?;
    Ok(unproven.map(|_|evidence))
}
fn canonical_for_rejection(root:&Path,evidence:&crate::bank_backup::Baseline)->Result<Option<crate::bank_backup::Baseline>> {
    let mut identity=evidence.udid.as_bytes().to_vec();identity.push(0);identity.extend_from_slice(evidence.card_hash.as_bytes());
    let path=root.join("bank-backups").join(crate::bank_backup::sha256(&identity)?);
    let canonical=match crate::bank_backup::validate_bundle_directory(&path) {Ok(value)=>value,Err(_)=>return Ok(None)};
    if !same_issuer(&canonical,evidence) {return Ok(None);}
    // The live store contains many cards. The remote-union validator rejects
    // mixed identities by design; validate every exact local operation instead.
    let history=root.join("bank-change-intents");
    let valid_history=(|| -> Result<()> {
        for directory in crate::bank_changes::cloud_operation_dirs(&history,&evidence.udid,&evidence.card_hash)? {
            let operation=directory.file_name().and_then(|name|name.to_str()).context("invalid local operation")?;
            crate::bank_changes::recovery_evidence_at(&history,operation,&evidence.udid,&evidence.card_hash)?;
        } Ok(())
    })();
    if valid_history.is_err() {return Ok(None);}
    let pending=crate::wallet_export::pending_recovery_operations_at(&root.join("wallet-export-recovery"))?;
    if crate::wallet_export::pending_recovery_blocks_device(Ok(&pending),Some(&evidence.udid)) {return Ok(None);}
    Ok(Some(canonical))
}
// Only an exhaustive positive disposition of matching cloud quarantine can
// release its review. Unknown bytes/issuer/history/phone returns stay blocked.
fn resolve_rejected_originals(root:&Path,owner:&VaultOwner,inventory:&std::collections::BTreeSet<String>,udid:&str,hash:&str)->Result<Option<(Vec<String>,usize)>> {
    let _lease=crate::storage::operation_lease_for_import(root)?;
    let quarantine=root.join("quarantine");crate::storage::safe_chain(&quarantine)?;
    if !quarantine.exists() {return Ok(None);}
    let mut archives=Vec::new();let mut rejected=0;let mut receipts=Vec::new();let mut canonical_digest=None;
    for entry in fs::read_dir(&quarantine)? {
        let path=entry?.path();if is_retained_alias_artifact(&path)? {continue;}
        if path.extension().and_then(|p|p.to_str())!=Some("zip") {return Ok(None);}
        let (manifest,bytes)=read_quarantine_archive(&path)?;
        let classified=classify_rejected_archive(root,&bytes)?;
        if manifest.udid!=udid || manifest.card_hash!=hash {continue;}
        let id=crate::bank_backup::sha256(&bytes)?;
        if !inventory.contains(&id) {return Ok(None);}
        if let Some(evidence)=classified {
            let Some(canonical)=canonical_for_rejection(root,&evidence)? else {return Ok(None);};
            let digest=crate::bank_backup::sha256(&serde_json::to_vec(&canonical)?)?;
            canonical_digest=Some(digest.clone());
            persist_exact(&root.join("rejected-originals").join(format!("{id}.zip")),&bytes)?;
            let disposition=serde_json::json!({"version":1,"owner":owner,"archive":id,"udid":udid,"card_hash":hash,
                "issuer":crate::bank_backup::sha256(&evidence.manifest_bytes)?,"issuer_artwork":crate::bank_backup::sha256(&evidence.remote_artwork_bytes)?,"canonical":digest,"disposition":"not_an_original_preserved"});
            persist_exact(&root.join("rejected-originals").join(format!("{id}.json")),&serde_json::to_vec(&disposition)?)?;
            rejected+=1;
        } else {
            let (files_manifest,files)=parse_bundle(&bytes)?;
            let canonical=materialize_inventory_archive(&root.join("original-review-evidence").join(&id),&files_manifest,&files)?;
            if canonical_for_rejection(root,&canonical)?.as_ref()!=Some(&canonical) {return Ok(None);}
            if import_bundle(root,&bytes).is_err() {return Ok(None);}
            receipts.push((id.clone(),manifest));
        }
        archives.push((id,path));
    }
    if rejected==0 || canonical_digest.is_none() {return Ok(None);}
    crate::storage::clear_card_review_at(root,udid,hash)?;
    let seen=seen_directory(root,owner)?;
    for (id,manifest) in receipts {write_seen(root,&seen.join(&id),&id,manifest)?;}
    let ids=archives.iter().map(|(id,_)|id.clone()).collect();
    for (_,path) in archives {fs::remove_file(path)?;}
    Ok(Some((ids,rejected)))
}

// This type is deliberately neither deserializable nor publicly constructible.
// It proves only fresh remote ownership selection, not physical restoration.
pub(crate) struct VerifiedRemoteRecovery {
    owner:VaultOwner,
    inventory_digest:String,
    archive_ids:Vec<String>,
    baseline:crate::bank_backup::Baseline,
    active:crate::bank_changes::Intent,
}
impl VerifiedRemoteRecovery {
    pub(crate) fn revalidate_owner_at(&self,root:&Path)->Result<()> {anyhow::ensure!(load_owner_at(root)?.as_ref()==Some(&self.owner),"recovery vault owner changed");Ok(())}
    pub(crate) fn baseline(&self)->&crate::bank_backup::Baseline {&self.baseline}
    pub(crate) fn active(&self)->&crate::bank_changes::Intent {&self.active}
    pub(crate) fn archive_ids(&self)->&[String] {&self.archive_ids}
    pub(crate) fn inventory_digest(&self)->&str {&self.inventory_digest}
    pub(crate) fn owner_digest(&self)->Result<String> {crate::bank_backup::sha256(&serde_json::to_vec(&self.owner)?)}
}
const RECOVERY_INVENTORY_BYTES:usize=512*1024*1024;
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct RecoveryInventoryEntry {id:String,sha256:String,size:usize}
#[derive(Deserialize)] #[serde(deny_unknown_fields)]
struct RecoveryInventory {bundles:Vec<RecoveryInventoryEntry>}

fn archive_from_evidence(manifest:&Manifest,files:&BTreeMap<String,Vec<u8>>)->Result<Vec<u8>> {
    let mut archive=zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).unix_permissions(0o600);
    archive.start_file("bundle.json",options)?;archive.write_all(&serde_json::to_vec(manifest)?)?;
    for (name,bytes) in files {archive.start_file(name,options)?;archive.write_all(bytes)?;}
    Ok(archive.finish()?.into_inner())
}
fn reconstruct_inventory_archive(root:&Path,owner:&VaultOwner,entry:&RecoveryInventoryEntry)->Result<Option<Vec<u8>>> {
    let path=seen_directory(root,owner)?.join(&entry.id);crate::storage::safe_chain(&path)?;
    let meta=match fs::symlink_metadata(&path) {Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(None),result=>result?};
    anyhow::ensure!(meta.is_file() && meta.len()<=1024*1024,"unsafe reconstruction receipt");
    let receipt:Receipt=match serde_json::from_slice(&fs::read(&path)?) {Ok(receipt)=>receipt,Err(_)=>return Ok(None)};
    if receipt.version!=1 || receipt.bundle_id!=entry.id {return Ok(None);}
    let mut files=BTreeMap::new();let mut total=0usize;
    for (name,record) in &receipt.manifest.files {
        anyhow::ensure!(safe_name(name) && evidence_name(name) && record.size<=32*1024*1024 && files.len()<MAX_FILES,"unsafe receipt evidence");
        total=total.checked_add(record.size).context("reconstruction size overflow")?;anyhow::ensure!(total<=LIMIT,"reconstruction too large");
        let (prefix,tail)=name.split_once('/').context("invalid receipt path")?;
        let path=root.join(if prefix=="bank" {"bank-backups"} else {"bank-change-intents"}).join(tail);
        crate::storage::safe_chain(&path)?;
        let meta=match fs::symlink_metadata(&path) {Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(None),result=>result?};
        anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=32*1024*1024,"unsafe reconstruction evidence");
        if meta.len()!=record.size as u64 {return Ok(None);}
        let bytes=fs::read(&path)?;if crate::bank_backup::sha256(&bytes)?!=record.sha256 {return Ok(None);}files.insert(name.clone(),bytes);
    }
    let bytes=archive_from_evidence(&receipt.manifest,&files)?;
    if bytes.len()==entry.size && crate::bank_backup::sha256(&bytes)?==entry.id {Ok(Some(bytes))} else {Ok(None)}
}
fn persist_exact(path:&Path,bytes:&[u8])->Result<()> {
    crate::storage::safe_chain(path)?;
    if path.exists() {anyhow::ensure!(fs::read(path)?==bytes,"immutable recovery evidence conflict");}
    else {crate::storage::write_new(path,bytes)?;}
    anyhow::ensure!(fs::read(path)?==bytes,"recovery evidence failed durable readback");Ok(())
}
fn materialize_inventory_archive(stage:&Path,manifest:&Manifest,files:&BTreeMap<String,Vec<u8>>)->Result<crate::bank_backup::Baseline> {
    let (baseline,unproven)=materialize_inventory_evidence(stage,manifest,files)?;
    if let Some(error)=unproven {return Err(error);}Ok(baseline)
}
fn materialize_inventory_evidence(stage:&Path,manifest:&Manifest,files:&BTreeMap<String,Vec<u8>>)->Result<(crate::bank_backup::Baseline,Option<anyhow::Error>)> {
    let mut bank_key=None;
    for (name,bytes) in files {
        let parts=name.split('/').collect::<Vec<_>>();anyhow::ensure!(parts.len()==3,"invalid recovery evidence depth");
        if parts[0]=="bank" {if let Some(key)=bank_key.as_ref() {anyhow::ensure!(key==parts[1],"mixed recovery baseline");}else {bank_key=Some(parts[1].to_string());}}
        persist_exact(&stage.join(name),bytes)?;
    }
    crate::storage::safe_chain(stage)?;
    let mut observed=BTreeMap::new();
    for entry in fs::read_dir(stage)? {
        let prefix=entry?.path();let name=prefix.file_name().and_then(|name|name.to_str()).context("invalid recovery prefix")?;
        anyhow::ensure!(matches!(name,"bank"|"intents"),"unexpected recovery staging content");
        crate::storage::safe_chain(&prefix)?;
        for entry in fs::read_dir(&prefix)? {
            let dir=entry?.path();crate::storage::safe_chain(&dir)?;
            let meta=fs::symlink_metadata(&dir)?;anyhow::ensure!(meta.is_dir() && !meta.file_type().is_symlink(),"unsafe recovery directory");
            files_in(&dir,&format!("{name}/{}",dir.file_name().unwrap().to_string_lossy()),&mut observed)?;
        }
    }
    anyhow::ensure!(&observed==files,"undeclared or contaminated recovery staging evidence");
    let (baseline,unproven)=match crate::bank_backup::validate_bundle_directory(&stage.join("bank").join(bank_key.context("recovery original missing")?)) {
        Ok(baseline)=>(baseline,None),
        Err(error)=>{let baseline=error.downcast_ref::<crate::bank_backup::UnprovenOriginal>().context("original self-integrity requires review")?.evidence().clone();(baseline,Some(error))}
    };
    anyhow::ensure!(baseline.udid==manifest.udid && baseline.card_hash==manifest.card_hash,"recovery archive identity mismatch");
    let intents=stage.join("intents");crate::storage::safe_chain(&intents)?;fs::create_dir_all(&intents)?;
    crate::bank_changes::active_cloud_intent_at(&intents,&manifest.udid,&manifest.card_hash)?;
    Ok((baseline,unproven))
}
// The transport is injectable only inside this module for synthetic regression.
// Production callers cannot issue this capability without current account auth.
fn prepare_remote_recovery_at(root:&Path,session:&Session,baseline:&crate::bank_backup::Baseline,
    request:&mut SyncTransport<'_>)->Result<VerifiedRemoteRecovery> {
    let owner=require_owner_at(root,session)?;
    anyhow::ensure!(crate::wallet_export::is_exact_card_hash(&baseline.card_hash),"invalid recovery identity");
    let headers=vec![("Authorization".into(),format!("Bearer {}",session.access_token))];
    let inventory=request("GET",&format!("{}/v1/bundles",session.server),&headers,&[],1024*1024)?;
    let listing:RecoveryInventory=serde_json::from_slice(&inventory)?;
    anyhow::ensure!(!listing.bundles.is_empty() && listing.bundles.len()<=1000,"recovery inventory count invalid");
    let mut total=0usize;let mut ids=std::collections::BTreeSet::new();
    for entry in &listing.bundles {
        anyhow::ensure!(entry.id.len()==64 && entry.id.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && entry.id==entry.sha256 && entry.size>0 && entry.size<=LIMIT && ids.insert(entry.id.clone()),"invalid recovery inventory entry");
        total=total.checked_add(entry.size).context("recovery inventory overflow")?;
        anyhow::ensure!(total<=RECOVERY_INVENTORY_BYTES,"Recovery inventory exceeds 512 MiB; no removal was authorized");
    }
    let inventory_digest=crate::bank_backup::sha256(&inventory)?;
    let identity_digest=crate::bank_backup::sha256(&serde_json::to_vec(&(owner.clone(),&baseline.udid,&baseline.card_hash))?)?;
    let validation=root.join("cloud-recovery-validated").join(&identity_digest).join(&inventory_digest);
    let union=validation.join("union");crate::storage::safe_chain(&union)?;fs::create_dir_all(&union)?;
    let mut expanded=0usize;let mut target_archives=Vec::new();let mut union_files=BTreeMap::new();
    for entry in listing.bundles {
        let cached=root.join("cloud-recovery-archives").join(format!("{}.zip",entry.id));
        crate::storage::safe_chain(&cached)?;
        let bytes=if cached.exists() {
            let meta=fs::symlink_metadata(&cached)?;anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()==entry.size as u64,"recovery archive cache requires review");fs::read(&cached)?
        }else if let Some(bytes)=retained_inventory_bundle(root,&entry.id,entry.size)? {bytes}
        else if let Some(bytes)=reconstruct_inventory_archive(root,&owner,&entry)? {bytes}
        else {request("GET",&format!("{}/v1/bundles/{}",session.server,entry.id),&headers,&[],LIMIT)?};
        anyhow::ensure!(bytes.len()==entry.size && crate::bank_backup::sha256(&bytes)?==entry.id,"recovery archive inventory mismatch");
        persist_exact(&cached,&bytes)?;
        let (manifest,files)=parse_bundle(&bytes)?;
        expanded=files.values().try_fold(expanded,|total,bytes|total.checked_add(bytes.len()).context("recovery expansion overflow"))?;
        anyhow::ensure!(expanded<=RECOVERY_INVENTORY_BYTES,"Expanded recovery evidence exceeds 512 MiB; no removal was authorized");
        let (remote,unproven)=materialize_inventory_evidence(&validation.join(&entry.id),&manifest,&files)?;
        if let Some(error)=unproven {
            persist_exact(&root.join("rejected-originals").join(format!("{}.zip",entry.id)),&bytes)?;
            if canonical_for_rejection(root,&remote)?.is_none() {return Err(error);}
            continue;
        }
        if manifest.udid!=baseline.udid || manifest.card_hash!=baseline.card_hash {continue;}
        anyhow::ensure!(&remote==baseline,"remote and local immutable originals differ");
        target_archives.push(entry.id);
        for (name,bytes) in files {if let Some(tail)=name.strip_prefix("intents/") {
            if let Some(previous)=union_files.insert(format!("intents/{tail}"),bytes.clone()) {anyhow::ensure!(previous==bytes,"conflicting remote operation revisions");}
            persist_exact(&union.join(tail),&bytes)?;
        }}
    }
    // Also reject files left in a formerly contaminated union: only bytes from
    // the current authenticated archives may influence operation selection.
    let mut observed=BTreeMap::new();
    for entry in fs::read_dir(&union)? {let dir=entry?.path();crate::storage::safe_chain(&dir)?;
        let meta=fs::symlink_metadata(&dir)?;anyhow::ensure!(meta.is_dir() && !meta.file_type().is_symlink(),"unsafe recovery union");
        files_in(&dir,&format!("intents/{}",dir.file_name().unwrap().to_string_lossy()),&mut observed)?;
    }
    anyhow::ensure!(observed==union_files,"undeclared recovery union evidence");
    let active=crate::bank_changes::active_cloud_intent_at(&union,&baseline.udid,&baseline.card_hash)?.context("No unique active applied remote operation; no removal was authorized")?;
    Ok(VerifiedRemoteRecovery {owner,inventory_digest,archive_ids:target_archives,baseline:baseline.clone(),active})
}
pub(crate) fn prepare_remote_recovery(session:&Session,udid:&str,hash:&str,mut progress:impl FnMut(&'static str))->Result<VerifiedRemoteRecovery> {
    anyhow::ensure!(!crate::storage::is_demo(),"demo cannot authorize recovery");
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(600);
    let remaining=||deadline.checked_duration_since(std::time::Instant::now()).filter(|d|!d.is_zero()).ok_or_else(||anyhow::Error::new(crate::auto_update::RequestTimeout));
    require_service(&session.server)?;progress("Verificando cuenta para recuperar el original…");
    let headers=vec![("Authorization".into(),format!("Bearer {}",session.access_token))];
    let result=crate::auto_update::http_request_with_budget("GET",&format!("{}/v1/auth/account",session.server),&headers,&[],65536,remaining()?);
    let auth=match result {
        Err(error) if error.downcast_ref::<crate::auto_update::HttpStatus>().is_some_and(|status|status.0==404)=>
            crate::auto_update::http_request_with_budget("GET",&format!("{}/v1/auth/session",session.server),&headers,&[],65536,remaining()?)?,
        result=>result?,
    };
    anyhow::ensure!(parse_account_license(session,&auth)?,SessionRejected);crate::storage::ensure_ready()?;
    let _lease=crate::storage::operation_lease()?;let root=crate::storage::root();
    let baseline=crate::bank_backup::load_baseline(udid,hash)?.context("verified local original required for recovery")?;
    progress("Revisando inventario y evidencia inmutable de la cuenta…");
    let proof=prepare_remote_recovery_at(&root,session,&baseline,&mut |method,url,headers,body,max| {
        let budget=remaining()?;
        if method=="GET" && max==LIMIT {progress("Descargando y verificando evidencia de recuperación…");crate::auto_update::http_download_bundle_with_budget(url,headers,max,budget)}
        else {crate::auto_update::http_request_with_budget(method,url,headers,body,max,budget)}
    })?;
    remaining()?;progress("Evidencia de cuenta verificada; todavía no se ha restaurado el iPhone.");Ok(proof)
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
#[cfg(test)] pub(crate) mod tests {
    use super::*;
    #[test]fn unfinished_history_explains_recovery_instead_of_reporting_a_download_failure(){
        let error=anyhow::Error::new(crate::bank_changes::PendingCloudHistory).context("synthetic private identity").context(SyncFailure::Import);
        let message=sync_failure_message(&error);
        assert!(message.contains("Restaurar original"));assert!(message.contains("Sincronizar ahora"));
        assert!(!message.contains("synthetic private"));assert!(retained_session_after_failure(&error));
    }
    #[test]
    #[ignore = "requires explicitly authorized read-only retained-import diagnostic"]
    fn diagnose_retained_imports_read_only() {
        if std::env::var("WALLET_STUDIO_IMPORT_DIAGNOSTIC").as_deref() != Ok("1") {
            println!("{{\"result\":\"explicit_diagnostic_required\"}}");
            return;
        }
        assert!(std::env::var_os("WALLET_STUDIO_TEST_STORAGE_ROOT").is_none()
            && std::env::var_os("WALLET_STUDIO_TEST_FIXTURE").is_none(), "diagnostic requires the real read-only profile");
        let root = crate::storage::root();
        assert!(crate::storage::safe_chain(&root).is_ok(), "diagnostic root requires review");
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        fn bump(counts: &mut BTreeMap<&str, usize>, category: &'static str) { *counts.entry(category).or_default() += 1; }
        fn matches_file(path: &Path, data: &[u8]) -> Result<Option<bool>> {
            crate::storage::safe_chain(path)?;
            let metadata = match fs::symlink_metadata(path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink(), "unsafe diagnostic evidence");
            Ok(Some(metadata.len() == data.len() as u64 && fs::read(path)? == data))
        }
        for (name, category) in [(".cloud-import.lock", "import_marker_present"), ("storage-blocked.json", "global_review_present"), (".migration.lock", "migration_marker_present")] {
            if root.join(name).exists() { bump(&mut counts, category); }
        }
        let operation_marker = root.join(".vault-operation.lock");
        if operation_marker.exists() {
            bump(&mut counts, "operation_marker_present");
            // Opening an existing file for shared read neither creates a lease
            // nor arms lock cleanup. An active exclusive lease denies this read.
            if crate::storage::safe_chain(&operation_marker).is_ok() && fs::File::open(&operation_marker).is_ok() { bump(&mut counts, "operation_marker_readable"); }
            else { bump(&mut counts, "operation_marker_unreadable"); }
        }
        let quarantine = root.join("quarantine");
        if quarantine.exists() {
            assert!(crate::storage::safe_chain(&quarantine).is_ok(), "diagnostic quarantine requires review");
            let entries = fs::read_dir(&quarantine).unwrap_or_else(|_| panic!("diagnostic quarantine unreadable"));
            for entry in entries.take(1001) {
                let path = match entry { Ok(entry) => entry.path(), Err(_) => { bump(&mut counts, "quarantine_unreadable"); continue; } };
                if path.extension().and_then(|value| value.to_str()) != Some("zip") { continue; }
                bump(&mut counts, "quarantine_archives");
                if counts["quarantine_archives"] > 1000 { bump(&mut counts, "diagnostic_count_limit"); break; }
                let metadata = fs::symlink_metadata(&path).ok();
                if crate::storage::safe_chain(&path).is_err() || !metadata.is_some_and(|meta| meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= LIMIT as u64) {
                    bump(&mut counts, "quarantine_unsafe"); continue;
                }
                let bytes = match fs::read(&path) { Ok(bytes) => bytes, Err(_) => { bump(&mut counts, "quarantine_read_failed"); continue; } };
                let digest = match crate::bank_backup::sha256(&bytes) { Ok(digest) => digest, Err(_) => { bump(&mut counts, "archive_checksum_failed"); continue; } };
                if path.file_stem().and_then(|name| name.to_str()) != Some(digest.as_str()) { bump(&mut counts, "archive_name_checksum_mismatch"); continue; }
                let (manifest, files) = match parse_bundle(&bytes) { Ok(parsed) => parsed, Err(_) => { bump(&mut counts, "archive_parse_failed"); continue; } };
                bump(&mut counts, "archive_parse_valid");
                let stage = root.join("cloud-staging").join(&digest);
                for (name, data) in &files {
                    match matches_file(&stage.join(name), data) {
                        Ok(Some(true)) => {}, Ok(Some(false)) => bump(&mut counts, "staging_evidence_mismatch"),
                        Ok(None) => bump(&mut counts, "staging_evidence_missing"), Err(_) => bump(&mut counts, "staging_evidence_unreadable"),
                    }
                    let (prefix, tail) = name.split_once('/').unwrap();
                    let destination = root.join(if prefix == "bank" { "bank-backups" } else { "bank-change-intents" }).join(tail);
                    match matches_file(&destination, data) {
                        Ok(Some(false)) => bump(&mut counts, "immutable_destination_conflict"), Err(_) => bump(&mut counts, "destination_unreadable"), _ => {},
                    }
                }
                let bank_keys: std::collections::BTreeSet<_> = files.keys().filter_map(|name| name.strip_prefix("bank/").and_then(|tail| tail.split('/').next())).collect();
                if bank_keys.len() != 1 { bump(&mut counts, "baseline_count_invalid"); continue; }
                match crate::bank_backup::validate_bundle_directory(&stage.join("bank").join(bank_keys.first().unwrap())) {
                    Ok(baseline) if baseline.udid == manifest.udid && baseline.card_hash == manifest.card_hash => bump(&mut counts, "staged_baseline_valid"),
                    Ok(_) => bump(&mut counts, "staged_baseline_identity_mismatch"),
                    Err(_) => bump(&mut counts, "staged_baseline_invalid"),
                }
                if crate::bank_changes::validate_cloud_chain(&stage.join("intents"), &manifest.udid, &manifest.card_hash).is_ok() { bump(&mut counts, "staged_history_valid"); }
                else { bump(&mut counts, "staged_history_invalid"); }
                match crate::bank_changes::cloud_operation_dirs(&root.join("bank-change-intents"), &manifest.udid, &manifest.card_hash) {
                    Err(error) => bump(&mut counts, if error.to_string() == "uncertain intent needs local recovery before cloud export" { "local_history_uncertain" } else { "local_history_requires_review" }),
                    Ok(directories) => {
                        let mut merged_bytes = files.values().map(Vec::len).sum::<usize>(); let mut merged_count = files.len();
                        for directory in directories {
                            let mut local_files = BTreeMap::new();
                            let prefix = format!("intents/{}", directory.file_name().unwrap().to_string_lossy());
                            if files_in(&directory, &prefix, &mut local_files).is_err() { bump(&mut counts, "local_history_unreadable"); continue; }
                            for (name, data) in local_files {
                                match matches_file(&stage.join(&name), &data) {
                                    Ok(Some(false)) => bump(&mut counts, "local_staged_history_conflict"),
                                    Ok(None) => {
                                        bump(&mut counts, "local_history_not_staged");
                                        merged_bytes = merged_bytes.saturating_add(data.len()); merged_count += 1;
                                    },
                                    Err(_) => bump(&mut counts, "local_staged_history_unreadable"), _ => {},
                                }
                            }
                        }
                        if merged_bytes > LIMIT || merged_count > MAX_FILES { bump(&mut counts, "merged_history_limit"); }
                    }
                }
            }
        }
        println!("{}", serde_json::json!({"result":"read_only_import_categories","counts":counts}));
    }
    #[test]
    #[ignore = "requires explicitly authorized saved-account read-only cloud diagnostic"]
    fn diagnose_saved_account_downloads_read_only() {
        if std::env::var("WALLET_STUDIO_CLOUD_DIAGNOSTIC").as_deref() != Ok("1") {
            println!("{{\"result\":\"explicit_diagnostic_required\"}}");
            return;
        }
        assert!(std::env::var_os("WALLET_STUDIO_TEST_STORAGE_ROOT").is_none()
            && std::env::var_os("WALLET_STUDIO_TEST_FIXTURE").is_none(),
            "diagnostic requires the real read-only profile");
        let session = match load_session() {
            Ok(Some(session)) => session,
            _ => panic!("saved diagnostic session unavailable"),
        };
        assert!(session.server == "https://wallet-studio-accounts.onrender.com",
            "diagnostic service is not the expected Wallet Studio backend");
        fn safe_error(error: &anyhow::Error) -> serde_json::Value {
            let status = error.downcast_ref::<crate::auto_update::HttpStatus>().map(|value| value.0);
            let category = if status.is_some() { "http" }
                else if error.downcast_ref::<crate::auto_update::RequestTimeout>().is_some() { "timeout" }
                else if error.chain().any(|cause| cause.to_string() == "Lectura de red incompleta") { "read" }
                else if error.chain().any(|cause| cause.to_string() == "Respuesta demasiado grande") { "size_limit" }
                else { "transport" };
            serde_json::json!({"category":category,"status":status})
        }
        let headers = [("Authorization".to_owned(), format!("Bearer {}", session.access_token))];
        let started = std::time::Instant::now();
        let inventory = match crate::auto_update::http_request(
            "GET", &format!("{}/v1/bundles", session.server), &headers, &[], 1024 * 1024,
        ) {
            Ok(bytes) => bytes,
            Err(error) => {
                println!("{}", serde_json::json!({"stage":"inventory","result":safe_error(&error),"elapsed_ms":started.elapsed().as_millis()}));
                panic!("read-only inventory diagnostic failed");
            }
        };
        #[derive(Deserialize)] struct DiagnosticEntry { id:String, sha256:String, size:usize }
        #[derive(Deserialize)] struct DiagnosticListing { bundles:Vec<DiagnosticEntry> }
        let listing: DiagnosticListing = match serde_json::from_slice(&inventory) {
            Ok(listing) => listing,
            Err(_) => panic!("diagnostic inventory format invalid"),
        };
        assert!(listing.bundles.len() <= 1000 && listing.bundles.iter().all(|entry|
            entry.id.len() == 64 && entry.id.bytes().all(|b| b.is_ascii_hexdigit())
                && entry.id == entry.sha256 && entry.size <= LIMIT), "diagnostic inventory outside bounds");
        println!("{}", serde_json::json!({"stage":"inventory","count":listing.bundles.len(),
            "minimum_size":listing.bundles.iter().map(|entry|entry.size).min(),
            "maximum_size":listing.bundles.iter().map(|entry|entry.size).max(),
            "total_size":listing.bundles.iter().map(|entry|entry.size as u64).sum::<u64>(),
            "elapsed_ms":started.elapsed().as_millis()}));
        let largest = listing.bundles.iter().enumerate().max_by_key(|(_,entry)|entry.size).map(|(index,_)|index);
        let mut verified_all = true;
        if let Some(index) = largest {
            let entry = &listing.bundles[index];
            let started = std::time::Instant::now();
            let result = match crate::auto_update::http_download_bundle(
                &format!("{}/v1/bundles/{}", session.server, entry.id), &headers, LIMIT,
            ) {
                Ok(bundle) => {
                    let verified = bundle.len() == entry.size
                        && crate::bank_backup::sha256(&bundle).is_ok_and(|digest|digest == entry.sha256);
                    verified_all &= verified;
                    serde_json::json!({"category":if verified {"verified"} else {"integrity"},
                        "expected_size":entry.size,"received_size":bundle.len(),"verified":verified})
                }
                Err(error) => { verified_all = false; safe_error(&error) }
            };
            println!("{}", serde_json::json!({"stage":"largest","result":result,"elapsed_ms":started.elapsed().as_millis()}));
        }
        assert!(verified_all, "read-only download diagnostic did not verify all selected bundles");
    }
    #[test]
    fn bundle_timeout_reports_retry_without_private_error_details_or_session_loss() {
        let error = anyhow::Error::new(crate::auto_update::RequestTimeout)
            .context("synthetic private diagnostic detail").context(SyncFailure::Download);
        let message = sync_failure_message(&error);
        assert!(message.contains("Las copias locales se conservan"));
        assert!(message.contains("Sincronizar ahora"));
        assert!(!message.contains("private diagnostic"));
        assert!(retained_session_after_failure(&error));
        assert!(authenticated_after_sync_failure(&error, true));
    }
    #[test] fn restored_device_session_survives_transient_startup_and_rejection_only_removes_exact_token() {
        let root=fixture_root();std::fs::create_dir_all(&root).unwrap();
        let current=Session {server:"https://example.test".into(),access_token:"synthetic-current-token".into(),user_id:"account-a".into()};
        let path=root.join("cloud-session.dpapi");let protected=protect(&serde_json::to_vec(&current).unwrap(),false).unwrap();
        crate::storage::write_new(&path,&protected).unwrap();
        let loaded=load_session_at(&root).unwrap().unwrap();assert_eq!(loaded.access_token,current.access_token);
        let offline=anyhow::anyhow!("timeout").context(SyncFailure::Session);
        assert!(retained_session_after_failure(&offline));assert!(!authenticated_after_sync_failure(&offline,false));
        assert_eq!(std::fs::read(&path).unwrap(),protected);
        let obsolete=Session {access_token:"synthetic-obsolete-token".into(),..current.clone()};
        forget_rejected_session_at(&root,&obsolete).unwrap();assert_eq!(std::fs::read(&path).unwrap(),protected);
        forget_rejected_session_at(&root,&current).unwrap();assert!(load_session_at(&root).unwrap().is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test] fn transient_sync_failures_do_not_log_out_a_verified_account() {
        let offline=anyhow::anyhow!("synthetic private transport detail").context(SyncFailure::Session);
        assert!(authenticated_after_sync_failure(&offline,true));
        assert!(!authenticated_after_sync_failure(&offline,false));
        assert!(retained_session_after_failure(&offline));
        assert!(!sync_failure_message(&offline).contains("private transport"));
        let unavailable=anyhow::Error::new(crate::auto_update::HttpStatus(503)).context(SyncFailure::Session);
        assert!(authenticated_after_sync_failure(&unavailable,true));
        assert!(sync_failure_message(&unavailable).contains("HTTP 503"));
        for code in [401,403] {
            for stage in [SyncFailure::Session,SyncFailure::Inventory,SyncFailure::Upload] {
                let rejected=anyhow::Error::new(crate::auto_update::HttpStatus(code)).context(stage);
                assert!(!authenticated_after_sync_failure(&rejected,true));
                assert!(!retained_session_after_failure(&rejected));
                assert!(sync_failure_message(&rejected).contains("servidor rechazó"));
            }
        }
        let wrong_identity=anyhow::Error::new(SessionRejected).context(SyncFailure::Session);
        assert!(!authenticated_after_sync_failure(&wrong_identity,true));
        assert!(!retained_session_after_failure(&wrong_identity));
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
    pub(crate) fn fixture_baseline()->crate::bank_backup::Baseline {
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
    pub(crate) fn proof_fixture(root:&Path,baseline:&crate::bank_backup::Baseline,bundles:&[Vec<u8>])->Result<VerifiedRemoteRecovery> {
        let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};
        bind_owner_at(root,&session,true)?;
        prepare_remote_recovery_at(root,&session,baseline,&mut |method,url,_,_,_| {
            anyhow::ensure!(method=="GET","unexpected proof mutation request");
            if url.ends_with("/v1/bundles") {return Ok(serde_json::to_vec(&serde_json::json!({"bundles":bundles.iter().map(|bytes| {
                let id=crate::bank_backup::sha256(bytes).unwrap();serde_json::json!({"id":id,"sha256":id,"size":bytes.len()})
            }).collect::<Vec<_>>()}))?);}
            bundles.iter().find(|bytes|crate::bank_backup::sha256(bytes).unwrap()==url.rsplit('/').next().unwrap()).cloned().context("unlisted request")
        })
    }
    #[test] fn recovery_proof_unions_immutable_snapshots_and_does_not_revive_old_applied_state() {
        let source=fixture_root();let target=fixture_root();let baseline=fixture_baseline();
        let bank=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&baseline).unwrap();
        let pairs=crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|leaf|(leaf.to_string(),if leaf.ends_with(".pdf") {b"%PDF-1.7\n%%EOF".to_vec()} else {baseline.remote_artwork_bytes.clone()})).collect::<Vec<_>>();
        let first=crate::bank_changes::save_fixture_applied_at(&source.join("bank-change-intents"),&baseline.udid,&baseline.card_hash,&pairs).unwrap();
        let applied=export_bundle(&source,&bank).unwrap();
        crate::bank_changes::restore_fixture_at(&first).unwrap();let restored=export_bundle(&source,&bank).unwrap();
        assert!(proof_fixture(&target,&baseline,&[applied.clone(),restored.clone()]).is_err(),"older applied revision grants no authority after restored revision");
        let next=crate::bank_changes::save_fixture_applied_at(&source.join("bank-change-intents"),&baseline.udid,&baseline.card_hash,&pairs).unwrap();
        let latest=export_bundle(&source,&bank).unwrap();
        let proof=proof_fixture(&target,&baseline,&[latest.clone(),applied.clone(),restored.clone()]).unwrap();
        assert_eq!(proof.active().operation_dir.file_name(),next.file_name());assert_eq!(proof.active().phase,"applied");
        assert_eq!(proof.baseline(),&baseline);assert_eq!(proof.archive_ids().len(),3);assert_eq!(proof.owner_digest().unwrap().len(),64);
        let mut changed=baseline.clone();changed.manifest_bytes.push(b' ');
        assert!(proof_fixture(&fixture_root(),&changed,&[latest.clone()]).is_err(),"exact baseline equality is mandatory");
        // Reusing byte-identical archive cache still requires a fresh inventory.
        let again=proof_fixture(&target,&baseline,&[latest.clone(),applied,restored]).unwrap();assert_eq!(again.active().assets,proof.active().assets);
        let other=fixture_root();let foreign=crate::bank_changes::save_fixture_applied_at(&other.join("bank-change-intents"),&baseline.udid,&baseline.card_hash,&pairs).unwrap();
        let foreign_bank=crate::bank_backup::save_fixture_at(&other.join("bank-backups"),&baseline).unwrap();let ambiguous=export_bundle(&other,&foreign_bank).unwrap();
        assert_ne!(foreign.file_name(),next.file_name());assert!(proof_fixture(&fixture_root(),&baseline,&[latest,ambiguous]).is_err(),"two active operations cannot be selected by timestamp");
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(target).unwrap();fs::remove_dir_all(other).unwrap();
    }
    #[test] fn recovery_proof_rejects_inventory_caps_duplicates_and_contaminated_isolated_stage() {
        let root=fixture_root();let baseline=fixture_baseline();let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};bind_owner_at(&root,&session,true).unwrap();
        for duplicate in [false,true] {
            let mut calls=0;
            let result=prepare_remote_recovery_at(&root,&session,&baseline,&mut |_,url,_,_,_| {
                calls+=1;assert!(url.ends_with("/v1/bundles"));
                let entries=(0..6).map(|index|{let id=if duplicate {"0".repeat(64)} else {format!("{index:064x}")};serde_json::json!({"id":id,"sha256":id,"size":LIMIT})}).collect::<Vec<_>>();
                Ok(serde_json::to_vec(&serde_json::json!({"bundles":entries})).unwrap())
            });assert!(result.is_err());assert_eq!(calls,1,"invalid/capped inventory must stop before fetching archives");
        }
        let source=fixture_root();let bank=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&baseline).unwrap();let bytes=export_bundle(&source,&bank).unwrap();let (manifest,files)=parse_bundle(&bytes).unwrap();
        let stage=root.join("isolated-synthetic-stage");materialize_inventory_archive(&stage,&manifest,&files).unwrap();
        crate::storage::write_new(&stage.join("bank").join(bank.file_name().unwrap()).join("cardBackgroundCombined.pdf"),b"%PDF-1.7\n%%EOF").unwrap();
        assert!(materialize_inventory_archive(&stage,&manifest,&files).is_err(),"undeclared staging bytes must never become remote ownership");
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(root).unwrap();
    }
    fn legacy_archive_fixture(input:&crate::bank_backup::Baseline)->Vec<u8> {
        let source=fixture_root();let bank=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),input).unwrap();
        let mut files=BTreeMap::new();files_in(&bank,&format!("bank/{}",bank.file_name().unwrap().to_string_lossy()),&mut files).unwrap();
        let records=files.iter().map(|(name,bytes)|(name.clone(),Record {size:bytes.len(),sha256:crate::bank_backup::sha256(bytes).unwrap()})).collect();
        let bytes=repack(Manifest {schema:1,udid:input.udid.clone(),card_hash:input.card_hash.clone(),files:records},files);
        fs::remove_dir_all(source).unwrap();bytes
    }
    #[test] fn rejected_original_uses_exact_local_history_in_a_mixed_card_store() {
        let root=fixture_root();let input=fixture_baseline();
        crate::bank_backup::save_fixture_at(&root.join("bank-backups"),&input).unwrap();
        let pairs=crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|leaf|(leaf.to_string(),if leaf.ends_with(".pdf") {b"%PDF-1.7\n%%EOF".to_vec()} else {input.remote_artwork_bytes.clone()})).collect::<Vec<_>>();
        let target=crate::bank_changes::save_fixture_applied_at(&root.join("bank-change-intents"),&input.udid,&input.card_hash,&pairs).unwrap();
        let other=crate::bank_changes::save_fixture_applied_at(&root.join("bank-change-intents"),"synthetic-other-device",&input.card_hash,&pairs).unwrap();
        assert!(canonical_for_rejection(&root,&input).unwrap().is_some(),"valid foreign histories must not prevent exact-card rejection resolution");
        // A self-consistent JSON edit to the target's historical metadata still
        // violates immutable revision coherence; it must not grant a disposition.
        let first=target.join("state-0001-prepared.json");let mut value:serde_json::Value=serde_json::from_slice(&fs::read(&first).unwrap()).unwrap();
        value["created_unix_ms"]=serde_json::json!(0);fs::write(first,serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(canonical_for_rejection(&root,&input).unwrap().is_none());assert!(other.exists());
        fs::remove_dir_all(root).unwrap();
    }
    fn partition_fixture(root:&Path)->(crate::bank_backup::Baseline,std::path::PathBuf,usize,Vec<std::path::PathBuf>) {
        partition_fixture_with_size(root,24000)
    }
    fn partition_fixture_with_size(root:&Path,payload_size:usize)->(crate::bank_backup::Baseline,std::path::PathBuf,usize,Vec<std::path::PathBuf>) {
        let input=fixture_baseline();let bank=crate::bank_backup::save_fixture_at(&root.join("bank-backups"),&input).unwrap();
        let mut seed=0x12345678u32;let mut operations=Vec::new();
        for index in 0..3 {
            let mut pdf=b"%PDF-1.7\n".to_vec();for _ in 0..payload_size {seed^=seed<<13;seed^=seed>>17;seed^=seed<<5;pdf.push(seed as u8);}pdf.extend_from_slice(b"\n%%EOF");
            let pairs=crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|leaf|(leaf.to_string(),if leaf.ends_with(".pdf") {pdf.clone()} else {input.remote_artwork_bytes.clone()})).collect::<Vec<_>>();
            let operation=crate::bank_changes::save_fixture_applied_at(&root.join("bank-change-intents"),&input.udid,&input.card_hash,&pairs).unwrap();
            if index<2 {crate::bank_changes::restore_fixture_at(&operation).unwrap();}operations.push(operation);
        }
        (input,bank,30000,operations)
    }
    #[test] fn partitioned_transport_handles_more_than_32_mib_of_incompressible_history() {
        let root=fixture_root();let (input,bank,_,_)=partition_fixture_with_size(&root,13*1024*1024);
        let original=export_bundle(&root,&bank).unwrap();assert!(original.len()>UPLOAD_LIMIT);
        let parts=export_bundles(&root,&bank,UPLOAD_LIMIT).unwrap();assert_eq!(parts.len(),2);
        let mut union=BTreeMap::new();for part in parts {
            assert!(part.len()<=UPLOAD_LIMIT);let (manifest,files)=parse_bundle(&part).unwrap();assert_eq!(manifest.udid,input.udid);
            materialize_inventory_archive(&root.join("test-verified-parts").join(crate::bank_backup::sha256(&part).unwrap()),&manifest,&files).unwrap();
            for (name,data) in files {if let Some(previous)=union.insert(name,data.clone()) {assert_eq!(previous,data);}}
        }
        assert_eq!(union,parse_bundle(&original).unwrap().1);fs::remove_dir_all(root).unwrap();
    }
    #[test] fn partitioned_cloud_bundles_preserve_all_operations_reverse_import_and_recovery() {
        let source=fixture_root();let target=fixture_root();let proof_root=fixture_root();let (input,bank,cap,operations)=partition_fixture(&source);
        let original=export_bundle(&source,&bank).unwrap();let (_,all)=parse_bundle(&original).unwrap();
        let parts=export_bundles(&source,&bank,cap).unwrap();assert_eq!(parts.len(),3);assert!(parts.iter().all(|part|part.len()<=cap));
        assert_eq!(parts,export_bundles(&source,&bank,cap).unwrap(),"stable parts retain exact receipts across retries");
        let mut union=BTreeMap::new();for part in parts.iter().rev() {
            let (manifest,files)=parse_bundle(part).unwrap();assert_eq!(manifest.udid,input.udid);assert_eq!(manifest.card_hash,input.card_hash);
            let op_names:std::collections::BTreeSet<_>=files.keys().filter_map(|name|name.strip_prefix("intents/").and_then(|tail|tail.split('/').next())).collect();assert_eq!(op_names.len(),1);
            for (name,data) in files {if let Some(old)=union.insert(name,data.clone()) {assert_eq!(old,data);}}
            import_bundle(&target,part).unwrap();import_bundle(&target,part).unwrap();
        }
        assert_eq!(union,all);assert_eq!(crate::bank_changes::cloud_operation_dirs(&target.join("bank-change-intents"),&input.udid,&input.card_hash).unwrap().len(),3);
        let reverse=parts.iter().rev().cloned().collect::<Vec<_>>();let proof=proof_fixture(&proof_root,&input,&reverse).unwrap();assert_eq!(proof.active().operation_dir.file_name(),operations.last().unwrap().file_name());
        assert!(export_bundles(&source,&bank,1).unwrap_err().downcast_ref::<LocalReview>().is_some());
        // An operation cannot be split even if every individual file would fit.
        let (_,files)=export_files(&source,&bank).unwrap();let baseline_only=files.iter().filter(|(name,_)|name.starts_with("bank/")).map(|(name,data)|(name.clone(),data.clone())).collect();
        let bank_size=encode_bundle(&input,&baseline_only,zip::CompressionMethod::Deflated).unwrap().len();
        let error=partition_bundles(&input,files,bank_size+100).unwrap_err();assert!(matches!(error.downcast_ref::<LocalReview>(),Some(LocalReview::OperationSize)));
        assert_eq!(export_bundle(&source,&bank).unwrap(),original,"failed export must preserve all evidence");
        for root in [source,target,proof_root] {fs::remove_dir_all(root).unwrap();}
    }
    #[test] fn partitioned_sync_resumes_after_one_put_without_claiming_complete() {
        let root=fixture_root();let (_,bank,cap,_)=partition_fixture(&root);let expected=export_bundles(&root,&bank,cap).unwrap();
        let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};bind_owner_at(&root,&session,true).unwrap();
        let mut remote=BTreeMap::<String,Vec<u8>>::new();let mut puts=0usize;
        for interrupted in [true,false] {
            let outcome=synchronize_owned_report_with_limit(&root,&session,&mut |_|{},&mut |method,url,_,body,_| {
                if method=="GET" && url.ends_with("/v1/bundles") {return Ok(serde_json::to_vec(&serde_json::json!({"bundles":remote.iter().map(|(id,data)|serde_json::json!({"id":id,"sha256":id,"size":data.len()})).collect::<Vec<_>>()})).unwrap());}
                if method=="GET" {return Ok(remote.get(url.rsplit('/').next().unwrap()).unwrap().clone());}
                assert_eq!(method,"PUT");assert!(body.len()<=cap);puts+=1;if interrupted && puts==2 {anyhow::bail!("synthetic interrupted upload");}
                let id=crate::bank_backup::sha256(body).unwrap();assert!(!remote.contains_key(&id),"completed part must not be uploaded twice");remote.insert(id.clone(),body.to_vec());
                Ok(serde_json::to_vec(&serde_json::json!({"id":id,"sha256":id,"size":body.len()})).unwrap())
            },cap);
            if interrupted {assert!(outcome.is_err());assert_eq!(remote.len(),1);}else{let report=outcome.unwrap();assert!(report.complete());assert_eq!(report.uploaded,2);}
        }
        assert_eq!(remote.len(),expected.len());for bytes in expected {assert_eq!(remote.get(&crate::bank_backup::sha256(&bytes).unwrap()),Some(&bytes));}
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn partitioned_inventory_converges_after_old_applied_then_new_active_then_restored() {
        for legacy in [false,true] {
        let source=fixture_root();let target=fixture_root();let (input,bank,cap,operations)=partition_fixture(&source);
        if legacy {for (index,operation) in operations.iter().enumerate() {for entry in fs::read_dir(operation).unwrap() {
            let path=entry.unwrap().path();if path.extension().and_then(|value|value.to_str())==Some("json") {
                let mut state:serde_json::Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();state["sequence"]=serde_json::json!(1);state["created_unix_ms"]=serde_json::json!(100+index);
                fs::write(path,serde_json::to_vec(&state).unwrap()).unwrap();
            }
        }}}
        let first=operations[0].file_name().unwrap().to_string_lossy();let (_,mut files)=export_files(&source,&bank).unwrap();
        files.retain(|name,_|name.starts_with("bank/") || (name.starts_with(&format!("intents/{first}/")) && !name.ends_with("originals_verified.json") && !name.ends_with("restored.json")));
        let old=encode_bundle(&input,&files,zip::CompressionMethod::Stored).unwrap();let parts=export_bundles(&source,&bank,cap).unwrap();
        // UUID directory order has no chronology. Force the adverse arrival:
        // old A applied, newest C applied, B restored, finally A restored.
        let ordered=operations.iter().rev().map(|operation| {
            let prefix=format!("intents/{}/",operation.file_name().unwrap().to_string_lossy());
            parts.iter().find(|part|parse_bundle(part).unwrap().1.keys().any(|name|name.starts_with(&prefix))).unwrap().clone()
        });
        let inventory=std::iter::once(old).chain(ordered).collect::<Vec<_>>();
        let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};bind_owner_at(&target,&session,true).unwrap();
        let report=synchronize_owned_report_with_limit(&target,&session,&mut |_|{},&mut |method,url,_,_,_| {
            assert_eq!(method,"GET","every current part is already present remotely");
            if url.ends_with("/v1/bundles") {return Ok(serde_json::to_vec(&serde_json::json!({"bundles":inventory.iter().map(|bytes|{let id=crate::bank_backup::sha256(bytes).unwrap();serde_json::json!({"id":id,"sha256":id,"size":bytes.len()})}).collect::<Vec<_>>()})).unwrap());}
            Ok(inventory.iter().find(|bytes|crate::bank_backup::sha256(bytes).unwrap()==url.rsplit('/').next().unwrap()).unwrap().clone())
        },cap).unwrap();
        assert!(report.complete(),"a complete valid inventory must converge despite an intermediate ambiguous union");
        assert_eq!(report.downloaded,inventory.len());assert!(!crate::storage::card_is_blocked_at(&target,&input.udid,&input.card_hash));
        let seen=seen_directory(&target,&owner_for(&session).unwrap()).unwrap();for bytes in &inventory {let id=crate::bank_backup::sha256(bytes).unwrap();assert!(verified_seen(&target,&seen.join(&id),&id).unwrap());}
        assert_eq!(crate::bank_changes::cloud_operation_dirs(&target.join("bank-change-intents"),&input.udid,&input.card_hash).unwrap().len(),3);
        for root in [source,target] {fs::remove_dir_all(root).unwrap();}
        }
    }
    #[test] fn rejected_legacy_original_is_preserved_while_later_canonical_and_other_device_sync() {
        for good_later in [false,true] {
            let target=fixture_root();let input=fixture_baseline();let mut bad=input.clone();
            bad.local_assets.insert(input.artwork_name.clone(),crate::bank_backup::LocalAssetState::Present {bytes:b"old custom design, not issuer bytes".to_vec()});
            let bad_zip=legacy_archive_fixture(&bad);let good_zip=legacy_archive_fixture(&input);
            let bad_id=crate::bank_backup::sha256(&bad_zip).unwrap();let good_id=crate::bank_backup::sha256(&good_zip).unwrap();
            let mut other=input.clone();other.udid="synthetic-other-healthy-device".into();
            crate::bank_backup::save_fixture_at(&target.join("bank-backups"),&other).unwrap();
            let pairs=crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|leaf|(leaf.to_string(),if leaf.ends_with(".pdf") {b"%PDF-1.7\n%%EOF".to_vec()} else {input.remote_artwork_bytes.clone()})).collect::<Vec<_>>();
            crate::bank_changes::save_fixture_applied_at(&target.join("bank-change-intents"),&other.udid,&other.card_hash,&pairs).unwrap();
            let aliases=br#"[{"hash":"invalid legacy hash / not identity authority","name":"Alias conservado"}]"#;
            let alias_path=target.join("quarantine").join(format!("aliases-{}.json",crate::bank_backup::sha256(aliases).unwrap()));
            crate::storage::write_new(&alias_path,aliases).unwrap();
            let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};bind_owner_at(&target,&session,true).unwrap();
            let mut downloads=Vec::new();let mut uploads=Vec::new();
            let report=synchronize_owned_report(&target,&session,&mut |_|{},&mut |method,url,_,body,_| {
                if method=="GET" && url.ends_with("/v1/bundles") {
                    let mut bundles=vec![serde_json::json!({"id":bad_id,"sha256":bad_id,"size":bad_zip.len()})];
                    if good_later {bundles.push(serde_json::json!({"id":good_id,"sha256":good_id,"size":good_zip.len()}));}
                    return Ok(serde_json::to_vec(&serde_json::json!({"bundles":bundles})).unwrap());
                }
                if method=="GET" {let id=url.rsplit('/').next().unwrap();downloads.push(id.to_string());return Ok(if id==bad_id {bad_zip.clone()}else{assert_eq!(id,good_id);good_zip.clone()});}
                uploads.push(parse_bundle(body).unwrap().0.udid);let id=crate::bank_backup::sha256(body).unwrap();
                Ok(serde_json::to_vec(&serde_json::json!({"id":id,"sha256":id,"size":body.len()})).unwrap())
            }).unwrap();
            assert_eq!(downloads.first(),Some(&bad_id));assert!(uploads.contains(&other.udid));
            assert_eq!(report.complete(),good_later);assert_eq!(report.rejected_originals,usize::from(good_later));
            assert_eq!(crate::storage::card_is_blocked_at(&target,&input.udid,&input.card_hash),!good_later);
            assert!(!target.join("storage-blocked.json").exists());
            assert_eq!(fs::read(target.join("rejected-originals").join(format!("{bad_id}.zip"))).unwrap(),bad_zip);
            let seen=seen_directory(&target,&owner_for(&session).unwrap()).unwrap();assert!(!seen.join(&bad_id).exists());
            if good_later {assert!(seen.join(&good_id).exists());assert!(report.message().contains("archivos antiguos no se usan como original"));assert!(target.join("rejected-originals").join(format!("{bad_id}.json")).is_file());}
            assert_eq!(fs::read(&alias_path).unwrap(),aliases,"an alias artifact is preserved, never imported or treated as original authority");
            fs::remove_dir_all(target).unwrap();
        }
    }
    #[test] fn alias_artifacts_never_hide_unknown_or_malformed_quarantine_evidence() {
        for case in ["unknown_name","wrong_digest","extra_field","wrong_shape","oversized_field","directory"] {
            let root=fixture_root();let input=fixture_baseline();crate::bank_backup::save_fixture_at(&root.join("bank-backups"),&input).unwrap();
            let mut bad=input.clone();bad.local_assets.insert(input.artwork_name.clone(),crate::bank_backup::LocalAssetState::Present {bytes:b"legacy custom bytes".to_vec()});
            let zip=legacy_archive_fixture(&bad);let id=crate::bank_backup::sha256(&zip).unwrap();
            crate::storage::write_new(&root.join("quarantine").join(format!("{id}.zip")),&zip).unwrap();
            let bytes=match case {
                "extra_field"=>br#"[{"hash":"invalid","name":"old","baseline":true}]"#.to_vec(),
                "wrong_shape"=>br#"{"hash":"invalid","name":"old"}"#.to_vec(),
                "oversized_field"=>serde_json::to_vec(&serde_json::json!([{"hash":"x".repeat(1025),"name":"old"}])).unwrap(),
                _=>br#"[{"hash":"invalid","name":"old"}]"#.to_vec(),
            };
            let name=match case {"unknown_name"=>"unknown.json".into(),"wrong_digest"=>format!("aliases-{}.json","0".repeat(64)),_=>format!("aliases-{}.json",crate::bank_backup::sha256(&bytes).unwrap())};
            let alias=root.join("quarantine").join(name);if case=="directory" {fs::create_dir(&alias).unwrap();}else{crate::storage::write_new(&alias,&bytes).unwrap();}
            assert!(!is_retained_alias_artifact(&alias).unwrap());crate::storage::block_card_at(&root,&input.udid,&input.card_hash).unwrap();
            let owner=VaultOwner {version:1,server:"https://synthetic.example.test".into(),user_id:"fixture-owner".into()};
            assert!(resolve_rejected_originals(&root,&owner,&[id.clone()].into_iter().collect(),&input.udid,&input.card_hash).unwrap().is_none());
            assert!(!reconcile_card_quarantine_at(&root,&input.udid,&input.card_hash,&mut |_|{}).unwrap());
            assert!(crate::storage::card_is_blocked_at(&root,&input.udid,&input.card_hash));assert!(alias.exists());assert_eq!(fs::read(root.join("quarantine").join(format!("{id}.zip"))).unwrap(),zip);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test] fn rejected_original_resolution_never_accepts_changed_issuer_or_unknown_quarantine() {
        let root=fixture_root();let input=fixture_baseline();crate::bank_backup::save_fixture_at(&root.join("bank-backups"),&input).unwrap();
        let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};bind_owner_at(&root,&session,true).unwrap();
        let mut bad=input.clone();bad.local_assets.insert(input.artwork_name.clone(),crate::bank_backup::LocalAssetState::Present {bytes:b"modified".to_vec()});
        bad.manifest_bytes.push(b' ');let changed=legacy_archive_fixture(&bad);let id=crate::bank_backup::sha256(&changed).unwrap();
        crate::storage::write_new(&root.join("quarantine").join(format!("{id}.zip")),&changed).unwrap();crate::storage::block_card_at(&root,&input.udid,&input.card_hash).unwrap();
        let inventory=[id.clone()].into_iter().collect();assert!(resolve_rejected_originals(&root,&owner_for(&session).unwrap(),&inventory,&input.udid,&input.card_hash).unwrap().is_none());
        assert!(crate::storage::card_is_blocked_at(&root,&input.udid,&input.card_hash));
        fs::remove_file(root.join("quarantine").join(format!("{id}.zip"))).unwrap();
        bad.manifest_bytes=input.manifest_bytes.clone();let known=legacy_archive_fixture(&bad);let known_id=crate::bank_backup::sha256(&known).unwrap();
        crate::storage::write_new(&root.join("quarantine").join(format!("{known_id}.zip")),&known).unwrap();let inventory=[known_id].into_iter().collect();
        fs::write(root.join("quarantine").join("unknown.zip"),b"invalid archive").unwrap();
        assert!(resolve_rejected_originals(&root,&owner_for(&session).unwrap(),&inventory,&input.udid,&input.card_hash).is_err());
        assert!(crate::storage::card_is_blocked_at(&root,&input.udid,&input.card_hash));fs::remove_dir_all(root).unwrap();
    }
    #[test] fn customer_cloud_import_conflict_does_not_block_other_downloads_or_local_uploads() {
        for prepared in [false,true] {
            let source=fixture_root();let target=fixture_root();let input=fixture_baseline();
            let first=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&input).unwrap();
            let bad=export_bundle(&source,&first).unwrap();let bad_id=crate::bank_backup::sha256(&bad).unwrap();
            let mut healthy=input.clone();healthy.udid="synthetic-healthy-remote".into();
            let second=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&healthy).unwrap();
            let good=export_bundle(&source,&second).unwrap();let good_id=crate::bank_backup::sha256(&good).unwrap();
            let existing=crate::bank_backup::save_fixture_at(&target.join("bank-backups"),&input).unwrap();
            let mut local=input.clone();local.udid="synthetic-local-upload".into();
            crate::bank_backup::save_fixture_at(&target.join("bank-backups"),&local).unwrap();
            let retained=if prepared {
                let pairs=crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|leaf|(leaf.to_string(),
                    if leaf.ends_with(".pdf") {b"%PDF-1.7\n%%EOF".to_vec()} else {input.remote_artwork_bytes.clone()})).collect::<Vec<_>>();
                let op=crate::bank_changes::save_fixture_prepared_at(&target.join("bank-change-intents"),&input.udid,&input.card_hash,&pairs).unwrap();
                op.join("state-0001-prepared.json")
            } else {fs::write(existing.join("remote-artwork.bin"),b"retained conflicting local bytes").unwrap();existing.join("remote-artwork.bin")};
            let retained_bytes=fs::read(&retained).unwrap();
            let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};
            bind_owner_at(&target,&session,true).unwrap();
            let mut downloads=Vec::new();let mut uploads=Vec::new();
            let report=synchronize_owned_report(&target,&session,&mut |_|{},&mut |method,url,_,body,_| {
                if method=="GET" && url.ends_with("/v1/bundles") {
                    return Ok(serde_json::to_vec(&serde_json::json!({"bundles":[
                        {"id":bad_id,"sha256":bad_id,"size":bad.len()},
                        {"id":good_id,"sha256":good_id,"size":good.len()}]})).unwrap());
                }
                if method=="GET" {let id=url.rsplit('/').next().unwrap();downloads.push(id.to_string());
                    return Ok(if id==bad_id {bad.clone()} else {assert_eq!(id,good_id);good.clone()});}
                assert_eq!(method,"PUT");let (manifest,_)=parse_bundle(body).unwrap();uploads.push(manifest.udid);
                let id=crate::bank_backup::sha256(body).unwrap();
                Ok(serde_json::to_vec(&serde_json::json!({"id":id,"sha256":id,"size":body.len()})).unwrap())
            }).unwrap();
            assert_eq!(downloads,vec![bad_id.clone(),good_id.clone()],"bad archive was processed before healthy archive");
            assert_eq!(uploads,vec![local.udid.clone()]);
            assert_eq!((report.remote_pending,report.local_pending,report.downloaded,report.uploaded),(1,1,1,1));
            assert!(!report.complete());assert!(report.message().contains("copias de la nube requieren revisión"));
            assert!(crate::storage::card_is_blocked_at(&target,&input.udid,&input.card_hash));
            assert!(!crate::storage::card_is_blocked_at(&target,&healthy.udid,&healthy.card_hash));
            assert_eq!(fs::read(&retained).unwrap(),retained_bytes);
            assert_eq!(fs::read(target.join("quarantine").join(format!("{bad_id}.zip"))).unwrap(),bad);
            let seen=seen_directory(&target,&owner_for(&session).unwrap()).unwrap();
            assert!(!seen.join(&bad_id).exists(),"failed import must not get a receipt");
            assert!(seen.join(&good_id).exists());
            assert_eq!(crate::bank_backup::validate_bundle_directory(&target.join("bank-backups").join(second.file_name().unwrap())).unwrap(),healthy);
            let repeated=synchronize_owned_report(&target,&session,&mut |_|{},&mut |method,url,_,body,_| {
                if method=="GET" {
                    assert!(url.ends_with("/v1/bundles"),"retained conflict must not be downloaded again");
                    return Ok(serde_json::to_vec(&serde_json::json!({"bundles":[
                        {"id":bad_id,"sha256":bad_id,"size":bad.len()},
                        {"id":good_id,"sha256":good_id,"size":good.len()}]})).unwrap());
                }
                let id=crate::bank_backup::sha256(body).unwrap();
                Ok(serde_json::to_vec(&serde_json::json!({"id":id,"sha256":id,"size":body.len()})).unwrap())
            }).unwrap();
            assert_eq!(repeated.remote_pending,1);assert!(!repeated.complete());assert!(!seen.join(&bad_id).exists());
            fs::remove_dir_all(source).unwrap();fs::remove_dir_all(target).unwrap();
        }
    }
    #[test] fn customer_cloud_older_compatible_snapshot_keeps_review_without_blocking_other_identities() {
        let source=fixture_root();let older_source=fixture_root();let target=fixture_root();let input=fixture_baseline();
        let old_path=crate::bank_backup::save_fixture_at(&older_source.join("bank-backups"),&input).unwrap();
        let old=export_bundle(&older_source,&old_path).unwrap();
        let mut differing=input.clone();differing.local_assets.insert(input.artwork_name.clone(),crate::bank_backup::LocalAssetState::Absent);
        let bad_path=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&differing).unwrap();
        let bad=export_bundle(&source,&bad_path).unwrap();
        let mut healthy=input.clone();healthy.udid="synthetic-healthy-remote".into();
        let good_path=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&healthy).unwrap();
        let good=export_bundle(&source,&good_path).unwrap();
        crate::bank_backup::save_fixture_at(&target.join("bank-backups"),&input).unwrap();
        let mut local=input.clone();local.udid="synthetic-local-upload".into();
        crate::bank_backup::save_fixture_at(&target.join("bank-backups"),&local).unwrap();
        let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};bind_owner_at(&target,&session,true).unwrap();
        let bundles=[bad,old,good].into_iter().map(|bytes|(crate::bank_backup::sha256(&bytes).unwrap(),bytes)).collect::<Vec<_>>();
        let mut downloaded=Vec::new();let mut uploaded=Vec::new();
        let report=synchronize_owned_report(&target,&session,&mut |_|{},&mut |method,url,_,body,_| {
            if method=="GET" && url.ends_with("/v1/bundles") {return Ok(serde_json::to_vec(&serde_json::json!({"bundles":bundles.iter().map(|(id,bytes)|serde_json::json!({"id":id,"sha256":id,"size":bytes.len()})).collect::<Vec<_>>()})).unwrap());}
            if method=="GET" {let id=url.rsplit('/').next().unwrap();downloaded.push(id.to_string());return Ok(bundles.iter().find(|(key,_)|key==id).unwrap().1.clone());}
            uploaded.push(parse_bundle(body).unwrap().0.udid);let id=crate::bank_backup::sha256(body).unwrap();
            Ok(serde_json::to_vec(&serde_json::json!({"id":id,"sha256":id,"size":body.len()})).unwrap())
        }).unwrap();
        assert_eq!(downloaded,bundles.iter().map(|(id,_)|id.clone()).collect::<Vec<_>>());assert_eq!(uploaded,vec![local.udid]);
        assert_eq!((report.remote_pending,report.downloaded,report.uploaded),(2,1,1));assert!(!report.complete());
        let seen=seen_directory(&target,&owner_for(&session).unwrap()).unwrap();
        for (id,bytes) in &bundles[..2] {assert!(!seen.join(id).exists());assert_eq!(fs::read(target.join("quarantine").join(format!("{id}.zip"))).unwrap(),*bytes);}
        assert!(crate::storage::card_is_blocked_at(&target,&input.udid,&input.card_hash));assert!(seen.join(&bundles[2].0).exists());
        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(older_source).unwrap();fs::remove_dir_all(target).unwrap();
    }
    #[test] fn customer_cloud_retained_inventory_cache_requires_exact_fresh_entry_and_preserves_bad_evidence() {
        let root=fixture_root();let bytes=b"synthetic retained evidence";let id=crate::bank_backup::sha256(bytes).unwrap();
        let path=root.join("quarantine").join(format!("{id}.zip"));crate::storage::write_new(&path,bytes).unwrap();
        assert_eq!(retained_inventory_bundle(&root,&id,bytes.len()).unwrap(),Some(bytes.to_vec()));
        assert!(retained_inventory_bundle(&root,&id,bytes.len()+1).is_err());
        assert!(retained_inventory_bundle(&root,&"0".repeat(64),bytes.len()).unwrap().is_none());
        fs::write(&path,b"tampered retained evidence").unwrap();
        assert!(retained_inventory_bundle(&root,&id,bytes.len()).is_err());
        assert_eq!(fs::read(&path).unwrap(),b"tampered retained evidence");
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn customer_cloud_import_isolation_never_swallows_transport_or_invalid_archive() {
        for malformed in [false,true] {
            let root=fixture_root();let session=Session {server:"https://synthetic.example.test".into(),access_token:"synthetic-token".into(),user_id:"fixture-owner".into()};
            bind_owner_at(&root,&session,true).unwrap();
            let bytes=b"malformed ZIP";let id=crate::bank_backup::sha256(bytes).unwrap();let mut calls=0;
            let error=synchronize_owned_report(&root,&session,&mut |_|{},&mut |method,url,_,_,_| {
                calls+=1;assert_eq!(method,"GET");
                if url.ends_with("/v1/bundles") {return Ok(serde_json::to_vec(&serde_json::json!({"bundles":[{"id":id,"sha256":id,"size":bytes.len()}]})).unwrap());}
                if malformed {Ok(bytes.to_vec())} else {Err(crate::auto_update::HttpStatus(401).into())}
            }).unwrap_err();
            assert_eq!(calls,2);assert!(!isolated_import_conflict(&error));
            if !malformed {assert_eq!(error.downcast_ref::<crate::auto_update::HttpStatus>().unwrap().0,401);}
            fs::remove_dir_all(root).unwrap();
        }
        let report=SyncReport {remote_pending:1,..Default::default()};
        assert!(!report.complete());assert!(!report.message().contains("No hay copias bancarias"));
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
    #[test] fn customer_quarantine_unknown_or_unrelated_evidence_never_clears_review() {
        let source=fixture_root(); let input=fixture_baseline();
        let bank=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&input).unwrap();
        let bundle=export_bundle(&source,&bank).unwrap();
        for scenario in ["missing", "empty", "malformed", "unreadable", "other_card"] {
            let target=fixture_root();
            crate::storage::block_card_at(&target,&input.udid,&input.card_hash).unwrap();
            let quarantine=target.join("quarantine");
            if scenario!="missing" {fs::create_dir_all(&quarantine).unwrap();}
            if scenario=="malformed" {
                let bytes=b"not a ZIP";
                fs::write(quarantine.join(format!("{}.zip",crate::bank_backup::sha256(bytes).unwrap())),bytes).unwrap();
            }
            if scenario=="unreadable" {fs::create_dir(quarantine.join("directory.zip")).unwrap();}
            if scenario=="other_card" {
                let (mut manifest,files)=parse_bundle(&bundle).unwrap();
                manifest.udid="synthetic-other-device".into();
                let bytes=repack(manifest,files);
                fs::write(quarantine.join(format!("{}.zip",crate::bank_backup::sha256(&bytes).unwrap())),bytes).unwrap();
            }
            let result=reconcile_card_quarantine_at(&target,&input.udid,&input.card_hash,&mut |_|{});
            if matches!(scenario,"malformed"|"unreadable") {assert!(result.is_err(),"{scenario}");}
            else {assert!(!result.unwrap(),"{scenario}");}
            assert!(crate::storage::card_is_blocked_at(&target,&input.udid,&input.card_hash),"{scenario}");
            assert!(!target.join("bank-backups").exists(),"no unrelated import: {scenario}");
            fs::remove_dir_all(target).unwrap();
        }
        fs::remove_dir_all(source).unwrap();
    }
    #[test] fn customer_quarantine_reconciles_and_clears_review_after_intent_is_restored() {
        let source=fixture_root();let target=fixture_root();let input=fixture_baseline();
        let source_path=crate::bank_backup::save_fixture_at(&source.join("bank-backups"),&input).unwrap();
        let _target_path=crate::bank_backup::save_fixture_at(&target.join("bank-backups"),&input).unwrap();
        let png=input.remote_artwork_bytes.clone();
        let pairs=crate::wallet_backup::CARD_ARTWORK_ASSETS.iter().map(|name|(name.to_string(),if name.ends_with(".pdf") {b"%PDF-1.7\n%%EOF".to_vec()} else {png.clone()})).collect::<Vec<_>>();
        let source_op=crate::bank_changes::save_fixture_applied_at(&source.join("bank-change-intents"),&input.udid,&input.card_hash,&pairs).unwrap();
        crate::bank_changes::restore_fixture_at(&source_op).unwrap();
        let bundle=export_bundle(&source,&source_path).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(25));

        // Target has an intent in prepared phase
        let target_op=crate::bank_changes::save_fixture_prepared_at(&target.join("bank-change-intents"),&input.udid,&input.card_hash,&pairs).unwrap();
        // Import fails because target intent is prepared -> quarantined and blocked
        assert!(import_bundle(&target,&bundle).is_err());
        assert!(crate::storage::card_is_blocked_at(&target,&input.udid,&input.card_hash));
        assert!(crate::storage::ensure_card_ready_at(&target,&input.udid,&input.card_hash).is_err());
        let digest=crate::bank_backup::sha256(&bundle).unwrap();
        assert!(target.join("quarantine").join(format!("{digest}.zip")).is_file());

        // Reconcile while still prepared does NOT clear review or delete quarantine
        let aliases=br#"[{"hash":"legacy-invalid","name":"Alias retenido"}]"#;
        let alias_path=target.join("quarantine").join(format!("aliases-{}.json",crate::bank_backup::sha256(aliases).unwrap()));crate::storage::write_new(&alias_path,aliases).unwrap();
        let mut log_lines=Vec::new();
        assert!(!reconcile_card_quarantine_at(&target,&input.udid,&input.card_hash,&mut |l| log_lines.push(l.to_string())).unwrap());
        assert!(crate::storage::card_is_blocked_at(&target,&input.udid,&input.card_hash));
        assert!(target.join("quarantine").join(format!("{digest}.zip")).is_file());

        // Target completes physical restoration: prepared -> applied -> restored
        crate::bank_changes::apply_fixture_at(&target_op).unwrap();
        crate::bank_changes::restore_fixture_at(&target_op).unwrap();

        // Now reconcile succeeds: imports bundle, deletes quarantine zip, and clears review!
        assert!(reconcile_card_quarantine_at(&target,&input.udid,&input.card_hash,&mut |l| log_lines.push(l.to_string())).unwrap());
        assert!(!crate::storage::card_is_blocked_at(&target,&input.udid,&input.card_hash));
        assert!(crate::storage::ensure_card_ready_at(&target,&input.udid,&input.card_hash).is_ok());
        assert!(!target.join("quarantine").join(format!("{digest}.zip")).exists());
        assert_eq!(fs::read(alias_path).unwrap(),aliases);

        fs::remove_dir_all(source).unwrap();fs::remove_dir_all(target).unwrap();
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
