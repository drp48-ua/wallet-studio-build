//! Version-independent, per-user storage. Migration only creates new files.
use anyhow::{Context, Result};
use std::{fs, io::Write, path::{Path, PathBuf}};
static DEMO_ROOT:std::sync::OnceLock<PathBuf>=std::sync::OnceLock::new();
pub fn is_demo()->bool { DEMO_ROOT.get().is_some() }
pub fn initialize_demo(path:&Path)->Result<()> {
    anyhow::ensure!(!path.exists() && path.is_absolute() && path.starts_with(std::env::temp_dir()),"demo needs a fresh temporary folder");
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
        if let Some(path)=std::env::var_os("WALLET_STUDIO_TEST_STORAGE_ROOT") { return PathBuf::from(path); }
    }
    local_folder().join("WalletStudio")
}

pub fn ensure_ready()->Result<()> {
    anyhow::ensure!(!root().join("storage-blocked.json").exists(),"Hay evidencia de almacenamiento en conflicto; conserva las copias y revisa la recuperación");
    anyhow::ensure!(!root().join(".migration.lock").exists() && !root().join(".cloud-import.lock").exists(),"El almacenamiento está siendo migrado o importado; espera o revisa la recuperación");
    Ok(())
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
    for ancestor in path.ancestors() { if let Ok(meta)=fs::symlink_metadata(ancestor) {
        anyhow::ensure!(!meta.file_type().is_symlink(),"storage path contains a link");
        #[cfg(windows)] { use std::os::windows::fs::MetadataExt; anyhow::ensure!(meta.file_attributes()&0x400==0,"storage reparse point is unsafe"); }
    }} Ok(())
}

pub(crate) fn block(root:&Path)->Result<()> {
    let path=root.join("storage-blocked.json");
    if !path.exists() { write_new(&path,b"{\"requires_review\":true}")?; } Ok(())
}

fn merge_tree(source:&Path, destination:&Path, root:&Path, critical:bool, report:&mut MigrationReport)->Result<()> {
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
                if critical { block(root)?; }
            }
        } else { write_new(destination,&bytes)?; report.copied+=1; }
    }
    Ok(())
}

pub fn migrate_legacy()->Result<MigrationReport> {
    if is_demo() { return Ok(MigrationReport::default()); }
    let destination=root(); fs::create_dir_all(&destination)?;
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
        for source in candidates { if source.is_dir() && source!=destination {
            for name in ["bank-backups","bank-change-intents","wallet-backups","wallet-export-recovery","transport-probe-recovery","cards.json","settings.json"] {
                let path=source.join(name); if path.exists() { merge_tree(&path,&destination.join(name),&destination,!matches!(name,"cards.json"|"settings.json"),&mut report)?; }
            }
        }}
        let banks=destination.join("bank-backups");
        if banks.is_dir() { for entry in fs::read_dir(&banks)? { let path=entry?.path();
            if crate::bank_backup::validate_bundle_directory(&path).is_err() { block(&destination)?; }
        }}
        anyhow::ensure!(!destination.join("storage-blocked.json").exists(),"La migración conserva evidencia en conflicto; requiere revisión"); Ok(report)
    })();
    drop(_file); if result.is_ok() { fs::remove_file(lock)?; }
    result
}

#[cfg(test)] mod tests {
    use super::*;
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
