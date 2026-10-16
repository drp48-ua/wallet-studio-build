//! Version-independent, per-user storage. Migration only creates new files.
use anyhow::{Context, Result};
use std::{fs, io::Write, path::{Path, PathBuf}};
thread_local! { static LEASE:std::cell::RefCell<Option<(PathBuf,usize,std::rc::Rc<LeaseFile>)>>=const {std::cell::RefCell::new(None)}; }
struct LeaseFile { root:PathBuf, file:Option<fs::File>,token:Vec<u8> }
impl Drop for LeaseFile {
    fn drop(&mut self) { self.file.take(); if !std::thread::panicking() {
        let path=self.root.join(".vault-operation.lock");
        if fs::read(&path).is_ok_and(|bytes|bytes==self.token) { let _=fs::remove_file(path); }
    }}
}
pub struct StorageLease { _file:std::rc::Rc<LeaseFile> }
impl Drop for StorageLease {
    fn drop(&mut self) {
        LEASE.with(|state| { let mut state=state.borrow_mut();
            if let Some((_,depth,_))=state.as_mut() { *depth-=1; if *depth==0 { *state=None; } } });
    }
}
pub fn operation_lease()->Result<StorageLease> { operation_lease_at(&root()) }
pub(crate) fn operation_lease_for_import(root:&Path)->Result<StorageLease> { operation_lease_at(root) }
fn operation_lease_at(root:&Path)->Result<StorageLease> {
    let nested=LEASE.with(|state| { let mut state=state.borrow_mut();
        if let Some((owner,depth,file))=state.as_mut() { anyhow::ensure!(owner==root,"storage root changed inside an operation"); *depth+=1; return Ok(Some(file.clone())); } Ok(None) })?;
    if let Some(file)=nested { return Ok(StorageLease {_file:file}); }
    safe_chain(root)?; fs::create_dir_all(root)?;
    let token=crate::flasher::generate_token().into_bytes();
    let mut file=fs::OpenOptions::new().write(true).create_new(true).open(root.join(".vault-operation.lock"))
        .context("Otra instancia usa las copias o quedó una operación interrumpida; no se iniciará otra transferencia")?;
    file.write_all(&token)?; file.sync_all()?;
    let file=std::rc::Rc::new(LeaseFile {root:root.into(),file:Some(file),token});
    LEASE.with(|state|*state.borrow_mut()=Some((root.into(),1,file.clone())));
    Ok(StorageLease {_file:file})
}
static DEMO_ROOT:std::sync::OnceLock<PathBuf>=std::sync::OnceLock::new();
pub fn is_demo()->bool { DEMO_ROOT.get().is_some() }
pub fn initialize_demo(path:&Path)->Result<()> {
    anyhow::ensure!(!path.exists() && path.is_absolute() && path.starts_with(std::env::temp_dir()),"demo needs a fresh temporary folder");
    safe_chain(path)?;
    fs::create_dir(path)?; DEMO_ROOT.set(path.to_path_buf()).map_err(|_|anyhow::anyhow!("demo already initialized"))?; Ok(())
}

#[derive(Default, Debug)]
pub struct MigrationReport { pub copied: usize, pub identical: usize, pub conflicts: usize }

#[cfg(windows)]
fn local_folder() -> PathBuf {
    #[repr(C)] struct Guid { a: u32, b: u16, c: u16, d: [u8;8] }
    #[link(name="shell32")] unsafe extern "system" { fn SHGetKnownFolderPath(id:*const Guid, flags:u32, token:*mut std::ffi::c_void, output:*mut *mut u16)->i32; }
    #[link(name="ole32")] unsafe extern "system" { fn CoTaskMemFree(pointer:*mut std::ffi::c_void); }
    let id=Guid { a:0xf1b32785,b:0x6fba,c:0x4fcf,d:[0x9d,0x55,0x7b,0x8e,0x7f,0x15,0x70,0x91] };
    let mut pointer=std::ptr::null_mut();
    let code=unsafe { SHGetKnownFolderPath(&id,0x00010000,std::ptr::null_mut(),&mut pointer) };
    assert!(code>=0 && !pointer.is_null(),"No se pudo resolver el almacenamiento personal; no se usará una carpeta compartida");
    let mut length=0; unsafe { while *pointer.add(length)!=0 { length+=1; assert!(length<32768); } }
    let value=unsafe { String::from_utf16_lossy(std::slice::from_raw_parts(pointer,length)) };
    unsafe { CoTaskMemFree(pointer.cast()); }
    PathBuf::from(value)
}
#[cfg(not(windows))]
fn local_folder()->PathBuf { PathBuf::from(std::env::var_os("HOME").expect("per-user home unavailable")).join(".local/share") }

pub fn root()->PathBuf {
    if let Some(path)=DEMO_ROOT.get() { return path.clone(); }
    #[cfg(test)]
    if std::env::var("WALLET_STUDIO_TEST_FIXTURE").as_deref()==Ok("1") {
        if let Some(path)=std::env::var_os("WALLET_STUDIO_TEST_STORAGE_ROOT") {
            let path=PathBuf::from(path);
            assert!(path.is_absolute() && path.starts_with(std::env::temp_dir()) && !path.components().any(|part|matches!(part,std::path::Component::ParentDir)),"fixture storage must stay in temporary storage");
            safe_chain(&path).expect("fixture path must have no links"); return path;
        }
    }
    local_folder().join("WalletStudio")
}

pub fn ensure_ready()->Result<()> {
    anyhow::ensure!(!root().join("storage-blocked.json").exists(),"Hay evidencia de almacenamiento en conflicto; conserva las copias y revisa la recuperación");
    anyhow::ensure!(!root().join(".migration.lock").exists() && !root().join(".cloud-import.lock").exists(),"El almacenamiento está siendo migrado o importado; espera o revisa la recuperación");
    let owns=LEASE.with(|state|state.borrow().as_ref().is_some_and(|(owner,_,_)|owner==&root()));
    anyhow::ensure!(owns || !root().join(".vault-operation.lock").exists(),"Otra instancia usa el archivo de originales o una operación interrumpida requiere revisión");
    Ok(())
}
fn card_key(udid:&str,hash:&str)->Result<String> {
    anyhow::ensure!(!udid.is_empty() && udid.len()<=256 && udid.trim()==udid && crate::wallet_export::is_exact_card_hash(hash),"invalid original identity");
    crate::bank_backup::sha256(&[udid.as_bytes(),b"\0",hash.as_bytes()].concat())
}
pub fn ensure_card_ready(udid:&str,hash:&str)->Result<()> { ensure_ready()?; ensure_card_ready_at(&root(),udid,hash) }
pub(crate) fn ensure_card_ready_at(root:&Path,udid:&str,hash:&str)->Result<()> {
    let path=root.join("card-review").join(card_key(udid,hash)?);safe_chain(&path)?;
    anyhow::ensure!(!path.exists(),"Esta tarjeta tiene evidencia en conflicto; conserva las copias y revisa su original");Ok(())
}
pub(crate) fn block_card_at(root:&Path,udid:&str,hash:&str)->Result<()> { block_key_at(root,&card_key(udid,hash)?) }
pub(crate) fn card_is_blocked_at(root:&Path,udid:&str,hash:&str)->bool {
    card_key(udid,hash).ok().map(|key|root.join("card-review").join(key).exists()).unwrap_or(false)
}
pub(crate) fn clear_card_review_at(root:&Path,udid:&str,hash:&str)->Result<()> {
    let key=card_key(udid,hash)?;
    let path=root.join("card-review").join(key);
    safe_chain(&path)?;
    if path.exists() { fs::remove_file(&path)?; }
    Ok(())
}
fn block_key_at(root:&Path,key:&str)->Result<()> {
    anyhow::ensure!(key.len()==64 && key.bytes().all(|b|b.is_ascii_hexdigit()),"invalid review key");
    let path=root.join("card-review").join(key);safe_chain(&path)?;
    if !path.exists() {write_new(&path,b"requires_review")?;}Ok(())
}

pub(crate) fn write_new(path:&Path, bytes:&[u8])->Result<()> {
    let parent=path.parent().context("missing storage parent")?;
    safe_chain(parent)?;
    fs::create_dir_all(parent)?;
    let meta=fs::symlink_metadata(parent)?;
    anyhow::ensure!(meta.is_dir() && !meta.file_type().is_symlink(),"unsafe storage directory");
    let mut file=fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?; file.sync_all()?;
    anyhow::ensure!(fs::read(path)?==bytes,"storage readback failed"); Ok(())
}
pub(crate) fn safe_chain(path:&Path)->Result<()> {
    for ancestor in path.ancestors() { match fs::symlink_metadata(ancestor) { Ok(meta)=> {
        anyhow::ensure!(!meta.file_type().is_symlink(),"storage path contains a link");
        #[cfg(windows)] { use std::os::windows::fs::MetadataExt; anyhow::ensure!(meta.file_attributes()&0x400==0,"storage reparse point is unsafe"); }
    }, Err(error) if error.kind()==std::io::ErrorKind::NotFound=>{},Err(error)=>return Err(error.into()) }} Ok(())
}

/// Mutable settings/credentials only. Immutable bank evidence never calls this.
#[cfg(windows)]
pub(crate) fn wide_path(path:&Path)->Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    let absolute=std::path::absolute(path)?;let mut value:Vec<u16>=absolute.as_os_str().encode_wide().collect();
    if !value.starts_with(&[92,92,63,92]) {
        let mut prefixed:Vec<u16>=if value.starts_with(&[92,92]) {"\\\\?\\UNC\\".encode_utf16().collect()} else {
            anyhow::ensure!(value.len()>=3 && value[1]==58 && value[2]==92,"storage path has no absolute Windows volume");
            "\\\\?\\".encode_utf16().collect()
        };
        if value.starts_with(&[92,92]) {prefixed.extend_from_slice(&value[2..]);} else {prefixed.extend_from_slice(&value);}
        value=prefixed;
    }
    anyhow::ensure!(!value.contains(&0),"storage path contains NUL");value.push(0);Ok(value)
}
pub(crate) fn replace_file_at(temporary:&Path,destination:&Path)->Result<()> {
    safe_chain(temporary)?; safe_chain(destination)?;
    #[cfg(windows)] {
        #[link(name="kernel32")] unsafe extern "system" { fn MoveFileExW(source:*const u16,target:*const u16,flags:u32)->i32; }
        let source=wide_path(temporary)?;let target=wide_path(destination)?;
        if unsafe { MoveFileExW(source.as_ptr(),target.as_ptr(),0x1|0x8) }==0 {
            return Err(std::io::Error::last_os_error()).context("No se pudo guardar la configuración; se conserva la versión anterior");
        }
    }
    #[cfg(not(windows))] { fs::rename(temporary,destination)?; }
    Ok(())
}

pub(crate) fn write_mutable(path:&Path,bytes:&[u8])->Result<()> {
    let parent=path.parent().context("settings parent missing")?;
    let temporary=parent.join(format!(".settings-{}",crate::flasher::generate_token()));
    write_new(&temporary,bytes)?; replace_file_at(&temporary,path)
}

pub(crate) fn block(root:&Path)->Result<()> {
    let path=root.join("storage-blocked.json");
    if !path.exists() { write_new(&path,b"{\"requires_review\":true}")?; } Ok(())
}

fn merge_tree(source:&Path, destination:&Path, root:&Path, critical:bool, report:&mut MigrationReport)->Result<()> {
    safe_chain(source)?; safe_chain(destination)?;
    let metadata=fs::symlink_metadata(source)?;
    anyhow::ensure!(!metadata.file_type().is_symlink(),"legacy symbolic link requires manual review");
    if metadata.is_dir() {
        for entry in fs::read_dir(source)? { let entry=entry?; merge_tree(&entry.path(),&destination.join(entry.file_name()),root,critical,report)?; }
    } else if metadata.is_file() {
        anyhow::ensure!(metadata.len()<=100*1024*1024,"legacy evidence file exceeds bound");
        let bytes=fs::read(source)?;
        if destination.exists() {
            let existing=fs::symlink_metadata(destination)?;
            anyhow::ensure!(existing.is_file() && !existing.file_type().is_symlink(),"destination evidence is unsafe");
            if fs::read(destination)?==bytes { report.identical+=1; }
            else {
                report.conflicts+=1;
                let digest=crate::bank_backup::sha256(&bytes)?;
                let quarantine=root.join("quarantine").join(digest);
                if !quarantine.exists() { write_new(&quarantine,&bytes)?; }
                if critical {
                    if let Ok(tail)=destination.strip_prefix(root.join("bank-backups")) {
                        if let Some(key)=tail.components().next().and_then(|part|part.as_os_str().to_str()).filter(|key|key.len()==64 && key.bytes().all(|b|b.is_ascii_hexdigit())) {block_key_at(root,key)?;} else {block(root)?;}
                    } else if let Ok(tail)=destination.strip_prefix(root.join("bank-change-intents")) {
                        let operation=tail.components().next().map(|part|root.join("bank-change-intents").join(part.as_os_str()));
                        match operation.and_then(|path|crate::bank_changes::operation_identity(&path).ok()) {
                            Some((udid,hash))=>block_card_at(root,&udid,&hash)?,None=>block(root)?,
                        }
                    } else {block(root)?;}
                }
            }
        } else { write_new(destination,&bytes)?; report.copied+=1; }
    }
    Ok(())
}

fn read_cards(path:&Path)->Result<Vec<crate::scanner::SavedCard>> {
    safe_chain(path)?; let meta=fs::symlink_metadata(path)?;
    anyhow::ensure!(meta.is_file() && meta.len()<=1024*1024,"card aliases invalid");
    let cards:Vec<crate::scanner::SavedCard>=serde_json::from_slice(&fs::read(path)?)?;
    anyhow::ensure!(cards.len()<=10000 && cards.iter().all(|card|crate::wallet_export::is_exact_card_hash(&card.hash) && card.name.len()<=256 && !card.name.chars().any(char::is_control)),"card alias fields invalid"); Ok(cards)
}
fn merged_cards(existing:&[crate::scanner::SavedCard],incoming:&[crate::scanner::SavedCard])->(Vec<crate::scanner::SavedCard>,usize) {
    let mut result=Vec::<crate::scanner::SavedCard>::new(); let mut conflicts=0;
    for card in existing.iter().chain(incoming) {
        if let Some(found)=result.iter().find(|known|known.hash==card.hash) { if found.name!=card.name { conflicts+=1; } }
        else { result.push(card.clone()); }
    } (result,conflicts)
}
fn merge_cards(source:&Path,destination:&Path,root:&Path,report:&mut MigrationReport)->Result<()> {
    safe_chain(source)?; safe_chain(destination)?;
    let result=(|| { let incoming=read_cards(source)?;
        let existing=if destination.exists() { read_cards(destination)? } else { Vec::new() };
        let (merged,conflicts)=merged_cards(&existing,&incoming); report.conflicts+=conflicts;
        if merged==existing { report.identical+=1; }
        else { write_mutable(destination,&serde_json::to_vec_pretty(&merged)?)?; report.copied+=1; }
        Ok::<(),anyhow::Error>(())
    })();
    if result.is_err() {
        // Alias failures never replace canonical names or certify an original.
        let meta=fs::symlink_metadata(source)?; anyhow::ensure!(meta.is_file() && meta.len()<=1024*1024,"alias file too large");
        let bytes=fs::read(source)?; let path=root.join("quarantine").join(format!("aliases-{}.json",crate::bank_backup::sha256(&bytes)?));
        if !path.exists() { write_new(&path,&bytes)?; } report.conflicts+=1;
    } Ok(())
}

pub fn migrate_legacy()->Result<MigrationReport> {
    if is_demo() { return Ok(MigrationReport::default()); }
    let _lease=operation_lease()?;
    let destination=root(); safe_chain(&destination)?; fs::create_dir_all(&destination)?;
    let lock=destination.join(".migration.lock");
    let _file=fs::OpenOptions::new().write(true).create_new(true).open(&lock)
        .context("La migración está en curso o requiere revisión")?;
    let result=(|| {
        let mut candidates=vec![local_folder().join("AirCard")];
        if let Some(path)=std::env::var_os("LOCALAPPDATA") { candidates.push(PathBuf::from(path).join("AirCard")); }
        if let Some(path)=std::env::var_os("USERPROFILE") { let profile=PathBuf::from(path);
            candidates.push(profile.join("AppData/Local/AirCard"));
            candidates.push(profile.join("AppData/Local/Packages/OpenAI.Codex_2p2nqsd0c76g0/LocalCache/Local/AirCard")); }
        candidates.sort(); candidates.dedup();
        let mut report=MigrationReport::default();
        for source in candidates { safe_chain(&source)?; if source.is_dir() && source!=destination {
            for name in ["bank-backups","bank-change-intents","wallet-backups","wallet-export-recovery","transport-probe-recovery","cards.json","settings.json"] {
                let path=source.join(name); if path.exists() {
                    if name=="cards.json" { merge_cards(&path,&destination.join(name),&destination,&mut report)?; }
                    else { merge_tree(&path,&destination.join(name),&destination,name!="settings.json",&mut report)?; }
                }
            }
        }}
        let banks=destination.join("bank-backups");
        if banks.is_dir() { for entry in fs::read_dir(&banks)? { let path=entry?.path();
            if crate::bank_backup::validate_bundle_directory(&path).is_err() {
                match crate::bank_backup::bundle_identity(&path) {Ok((udid,hash))=>block_card_at(&destination,&udid,&hash)?,Err(_)=>block(&destination)?,}
            }
        }}
        if crate::bank_changes::review_persistence_store_at(&destination.join("bank-change-intents"),&destination).is_err() {
            block(&destination)?;
        }
        if crate::wallet_export::pending_recovery_operations().is_err() { block(&destination)?; }
        anyhow::ensure!(!destination.join("storage-blocked.json").exists(),"La migración conserva evidencia en conflicto; requiere revisión"); Ok(report)
    })();
    drop(_file); if result.is_ok() { fs::remove_file(lock)?; }
    result
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn customer_lease_excludes_threads_and_retains_nested_guard() {
        let base=std::env::temp_dir().join(format!("ws-lease-{}",crate::flasher::generate_token()));
        let outer=operation_lease_at(&base).unwrap(); let inner=operation_lease_at(&base).unwrap();
        drop(outer); assert!(base.join(".vault-operation.lock").exists());
        let other=base.clone(); assert!(std::thread::spawn(move||operation_lease_at(&other).is_err()).join().unwrap());
        drop(inner); assert!(!base.join(".vault-operation.lock").exists());
        let next=operation_lease_at(&base).unwrap(); drop(next); fs::remove_dir_all(base).unwrap();
    }
    #[test] fn customer_alias_union_preserves_canonical_name_and_is_idempotent() {
        use crate::scanner::SavedCard;
        let hash="OM6NYhwXMZrAw0sRUjR62wmF4ZQ=".to_string();
        let existing=vec![SavedCard { hash:hash.clone(),name:"Nombre conservado".into() }];
        let incoming=vec![SavedCard { hash:hash.clone(),name:"Otro nombre".into() },SavedCard {hash:hash.clone(),name:"Duplicado".into()}];
        let (merged,conflicts)=merged_cards(&existing,&incoming);
        assert_eq!(merged.len(),1); assert_eq!(merged[0].name,"Nombre conservado"); assert_eq!(conflicts,2);
        let extra=SavedCard {hash:"AAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),name:"Tarjeta nueva".into()};
        let (union,conflicts)=merged_cards(&merged,&[extra]);assert_eq!(union.len(),2);assert_eq!(conflicts,0);
        let (again,conflicts)=merged_cards(&union,&union); assert_eq!(again.len(),2); assert_eq!(conflicts,0);
    }
    #[test] fn customer_bank_migration_conflict_is_scoped_to_its_identity_key() {
        let base=std::env::temp_dir().join(format!("ws-migrate-fixture-{}",crate::flasher::generate_token()));
        let root=base.join("new");let hash="OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";let key=card_key("synthetic-A",hash).unwrap();
        let source=base.join("old/asset");let destination=root.join("bank-backups").join(&key).join("remote-artwork.bin");
        write_new(&source,b"incoming conflicting fixture").unwrap();write_new(&destination,b"existing original fixture").unwrap();
        let mut report=MigrationReport::default();merge_tree(&source,&destination,&root,true,&mut report).unwrap();
        assert_eq!(fs::read(&destination).unwrap(),b"existing original fixture");assert!(ensure_card_ready_at(&root,"synthetic-A",hash).is_err());
        assert!(ensure_card_ready_at(&root,"synthetic-B",hash).is_ok());assert!(!root.join("storage-blocked.json").exists());fs::remove_dir_all(base).unwrap();
    }
    #[test] fn customer_card_review_clears_safely_and_is_scoped() {
        let root=std::env::temp_dir().join(format!("ws-review-fixture-{}",crate::flasher::generate_token()));
        let hash="OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        block_card_at(&root,"device-A",hash).unwrap();
        assert!(card_is_blocked_at(&root,"device-A",hash));
        assert!(ensure_card_ready_at(&root,"device-A",hash).is_err());
        assert!(!card_is_blocked_at(&root,"device-B",hash));
        assert!(ensure_card_ready_at(&root,"device-B",hash).is_ok());
        clear_card_review_at(&root,"device-A",hash).unwrap();
        assert!(!card_is_blocked_at(&root,"device-A",hash));
        assert!(ensure_card_ready_at(&root,"device-A",hash).is_ok());
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn customer_atomic_replace_failure_preserves_existing_settings() {
        let base=std::env::temp_dir().join(format!("ws-atomic-{}",crate::flasher::generate_token()));
        let destination=base.join("session"); write_new(&destination,b"existing protected session").unwrap();
        assert!(replace_file_at(&base.join("missing"),&destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(),b"existing protected session"); fs::remove_dir_all(base).unwrap();
    }
    #[cfg(windows)] #[test] fn customer_atomic_metadata_replace_supports_long_windows_paths() {
        let root=std::env::temp_dir().join(format!("ws-long-fixture-{}",crate::flasher::generate_token()));
        let path=root.join("a".repeat(100)).join("b".repeat(100)).join("receipt.json");assert!(path.as_os_str().len()>260);
        write_mutable(&path,b"first receipt").unwrap();write_mutable(&path,b"replacement receipt").unwrap();
        assert_eq!(fs::read(path).unwrap(),b"replacement receipt");fs::remove_dir_all(root).unwrap();
    }
    #[test] fn migration_is_create_only_and_conflicts_preserve_both() {
        let base=std::env::temp_dir().join(format!("ws-storage-{}",crate::flasher::generate_token()));
        let source=base.join("old/a"); let destination=base.join("new/a");
        write_new(&source,b"original").unwrap(); let mut report=MigrationReport::default();
        merge_tree(&source,&destination,&base.join("new"),true,&mut report).unwrap();
        merge_tree(&source,&destination,&base.join("new"),true,&mut report).unwrap();
        fs::write(&source,b"incoming").unwrap();
        merge_tree(&source,&destination,&base.join("new"),true,&mut report).unwrap();
        assert_eq!(fs::read(destination).unwrap(),b"original"); assert_eq!(report.copied,1); assert_eq!(report.identical,1); assert_eq!(report.conflicts,1);
        assert!(base.join("new/storage-blocked.json").is_file()); fs::remove_dir_all(base).unwrap();
    }
}
