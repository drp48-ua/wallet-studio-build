//! Full AirTraffic exercise using only two owned, ordinary Media files.
//! The durable Books preimage permits recovery independently of the sync result.
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use anyhow::{Context, Result};
use crate::airlift::{BooksSnapshot, build_books_plist, restore_books, snapshot_books};
use crate::device::{ActiveDeviceSession, ConnectionMode, DeviceTransport};
use crate::afc::AfcClient;

const DATA: &[u8] = b"Wallet Studio: synthetic Media transfer, no card data.\n";

#[derive(serde::Serialize, serde::Deserialize)]
struct ProbeJournal {
    version: u32,
    token: String,
    udid: String,
    books: HashMap<String, Option<Vec<u8>>>,
}

fn root() -> PathBuf {
    crate::storage::root().join("transport-probe-recovery")
}

const COMPLETED_PROBE_MARKER:&[u8]=b"Books verified; owned Media files absent; no card file touched.\n";
const MAX_PASSIVE_PROBE_JOURNAL:u64=8*1024*1024;
fn read_passive_probe_file(path:&std::path::Path,maximum:u64)->Result<Vec<u8>> {
    use std::io::Read;
    crate::storage::safe_chain(path)?;let before=fs::symlink_metadata(path)?;
    anyhow::ensure!(before.is_file() && !before.file_type().is_symlink() && before.len()<=maximum,"Registro histórico de transporte fuera de formato o límites");
    let mut bytes=Vec::new();fs::File::open(path)?.take(maximum+1).read_to_end(&mut bytes)?;
    let after=fs::symlink_metadata(path)?;
    anyhow::ensure!(bytes.len() as u64==before.len() && after.len()==before.len() && after.modified()?==before.modified()?,"El registro histórico cambió durante su revisión");Ok(bytes)
}
/// Classifies retained historical completion records only. Legacy journals did
/// not store an independent historical SHA; current hashes prove stable reads,
/// not authenticity, original artwork, or today's Books state on any phone.
pub(crate) fn passive_completed_probe_count_at(root:&std::path::Path)->Result<usize> {
    crate::storage::safe_chain(root)?;
    if !root.exists() {return Ok(0);}
    anyhow::ensure!(fs::symlink_metadata(root)?.is_dir(),"Área histórica de transporte inválida");
    let mut count=0;
    for entry in fs::read_dir(root)?.take(101) {
        anyhow::ensure!(count<100,"Demasiados registros históricos de transporte");
        let operation=entry?.path();crate::storage::safe_chain(&operation)?;
        anyhow::ensure!(fs::symlink_metadata(&operation)?.is_dir(),"Operación histórica inválida");
        let token=operation.file_name().and_then(|n|n.to_str()).context("Token histórico inválido")?;names(token)?;
        let mut leaves=std::collections::BTreeSet::new();
        for entry in fs::read_dir(&operation)?.take(3) {
            let path=entry?.path();crate::storage::safe_chain(&path)?;
            let name=path.file_name().and_then(|n|n.to_str()).context("Nombre histórico inválido")?;
            anyhow::ensure!(matches!(name,"books-preimage.json"|"recovered.txt") && leaves.insert(name.to_string()),"Contenido histórico desconocido");
        }
        anyhow::ensure!(leaves.len()==2,"La prueba histórica está incompleta; conserva su recuperación");
        let path=operation.join("books-preimage.json");let bytes=read_passive_probe_file(&path,MAX_PASSIVE_PROBE_JOURNAL)?;
        let value:serde_json::Value=serde_json::from_slice(&bytes)?;
        let object=value.as_object().context("Registro histórico inválido")?;
        anyhow::ensure!(object.len()==4 && ["version","token","udid","books"].iter().all(|key|object.contains_key(*key)),"Campos históricos desconocidos");
        let journal:ProbeJournal=serde_json::from_value(value)?;
        anyhow::ensure!(journal.version==1 && journal.token==token && !journal.udid.is_empty() && journal.udid.len()<=256
            && journal.udid.bytes().all(|b|b.is_ascii_alphanumeric() || b==b'-'),"Identidad histórica inválida");
        anyhow::ensure!(journal.books.len()==crate::airlift::TRACKED_BOOKS_FILES.len()
            && crate::airlift::TRACKED_BOOKS_FILES.iter().all(|name|journal.books.contains_key(*name))
            && journal.books.values().flatten().all(|data|data.len()<=4*1024*1024)
            && journal.books.values().flatten().map(Vec::len).sum::<usize>()<=4*1024*1024,"Preimagen histórica fuera del contrato");
        let marker_path=operation.join("recovered.txt");let marker=read_passive_probe_file(&marker_path,COMPLETED_PROBE_MARKER.len() as u64)?;
        anyhow::ensure!(marker==COMPLETED_PROBE_MARKER,"La finalización histórica no está acreditada");
        let current_hash=crate::bank_backup::sha256(&bytes)?;
        anyhow::ensure!(crate::bank_backup::sha256(&read_passive_probe_file(&path,MAX_PASSIVE_PROBE_JOURNAL)?)?==current_hash
            && read_passive_probe_file(&marker_path,COMPLETED_PROBE_MARKER.len() as u64)?==marker,"El registro histórico cambió durante la revisión");
        count+=1;
    }Ok(count)
}

fn names(token: &str) -> Result<(String, String)> {
    anyhow::ensure!(token.len() == 20 && token.bytes().all(|byte| byte.is_ascii_hexdigit()), "Token de prueba inválido");
    Ok((format!("WalletStudio-transport-{token}-source.bin"), format!("WalletStudio-transport-{token}-result.bin")))
}

fn recover(journal: &ProbeJournal) -> Result<()> {
    anyhow::ensure!(journal.version == 1 && journal.books.len() == crate::airlift::TRACKED_BOOKS_FILES.len()
        && crate::airlift::TRACKED_BOOKS_FILES.iter().all(|path| journal.books.contains_key(*path)), "Registro auxiliar de prueba inválido");
    let (source, target) = names(&journal.token)?;
    let session = ActiveDeviceSession::open(Some(&journal.udid), ConnectionMode::Usb)?;
    let afc = AfcClient::new(&session)?;
    let snapshot = BooksSnapshot { files: journal.books.clone() };
    restore_books(&afc, &snapshot)?;
    anyhow::ensure!(snapshot_books(&afc)?.files == snapshot.files, "No se verificó la recuperación de Libros");
    for path in [source, target] {
        if afc.exists(&path) {
            anyhow::ensure!(afc.read_file(&path)? == DATA, "Contenido inesperado en archivo de prueba; se conserva");
            afc.remove_path(&path)?;
            anyhow::ensure!(!afc.exists(&path), "Archivo temporal sigue presente");
        }
    }
    drop(afc);
    drop(session);
    std::thread::sleep(std::time::Duration::from_secs(2));
    let fresh = ActiveDeviceSession::open(Some(&journal.udid), ConnectionMode::Usb)?;
    anyhow::ensure!(snapshot_books(&AfcClient::new(&fresh)?)?.files == snapshot.files, "Libros cambió después de recuperar");
    Ok(())
}

pub fn run_if_requested() -> bool {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else { return false };
    if !matches!(command, "--wallet-studio-probe-media" | "--wallet-studio-recover-media-probe") { return false; }
    let result = (|| -> Result<()> {
        if command == "--wallet-studio-recover-media-probe" {
            anyhow::ensure!(args.len() == 2, "Indica el token de recuperación");
            names(&args[1])?;
            let path = root().join(&args[1]).join("books-preimage.json");
            let journal: ProbeJournal = serde_json::from_slice(&fs::read(&path)?)?;
            anyhow::ensure!(journal.token == args[1], "Registro de otro token");
            recover(&journal)?;
            println!("MEDIA_RECOVERED: auxiliares verificados; ninguna tarjeta modificada.");
            return Ok(());
        }
        anyhow::ensure!(args.len() == 1, "Argumentos adicionales no permitidos");
        let devices = crate::device::list_connected_devices()?;
        let devices: Vec<_> = devices.iter().filter(|item| item.has_transport(DeviceTransport::Usb)).collect();
        anyhow::ensure!(devices.len() == 1, "Conecta un único iPhone por USB");
        let device = devices[0];
        crate::device::ensure_wallet_version_supported(&device.udid)?;
        let token = crate::flasher::generate_token();
        let (source, target) = names(&token)?;
        let session = ActiveDeviceSession::open(Some(&device.udid), ConnectionMode::Usb)?;
        let afc = AfcClient::new(&session)?;
        anyhow::ensure!(!afc.exists(&source) && !afc.exists(&target), "Colisión de prueba; no se escribe");
        let snapshot = snapshot_books(&afc)?;
        let journal = ProbeJournal { version: 1, token: token.clone(), udid: device.udid.clone(), books: snapshot.files };
        fs::create_dir_all(root())?;
        let operation = root().join(&token);
        fs::create_dir(&operation)?;
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(operation.join("books-preimage.json"))?;
        let bytes = serde_json::to_vec(&journal)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        anyhow::ensure!(fs::read(operation.join("books-preimage.json"))? == bytes, "La preimagen PC difiere");
        println!("Dispositivo: {} · iOS {}", device.product_type, device.ios_version);
        println!("MEDIA_RECOVERY_TOKEN: {token}");
        let transfer = (|| -> Result<()> {
            afc.write_file(&source, DATA)?;
            anyhow::ensure!(afc.read_file(&source)? == DATA, "Archivo de prueba no coincide");
            let identifier = format!("../../{source}");
            afc.make_directory_recursive("Books/Sync")?;
            afc.write_file("Books/Sync/Books.plist", &build_books_plist(&[identifier.clone()])?)?;
            crate::airtraffic::sync_assets_via_airtraffic(&device.udid, DeviceTransport::Usb,
                &[(identifier.as_str(), target.as_str())], |line| println!("{line}"))
        })();
        drop(afc);
        drop(session);
        let fresh = ActiveDeviceSession::open(Some(&device.udid), ConnectionMode::Usb)?;
        let reader = AfcClient::new(&fresh)?;
        let copied = reader.read_file(&target).is_ok_and(|bytes| bytes == DATA);
        let source_consumed = !reader.exists(&source);
        drop(reader);
        drop(fresh);
        println!("MEDIA_OBSERVATION: bytesMatch={copied}; sourceConsumed={source_consumed}; protocolConfirmed={}", transfer.is_ok());
        recover(&journal).context(format!("Recuperación pendiente: {token}"))?;
        fs::write(operation.join("recovered.txt"), COMPLETED_PROBE_MARKER)?;
        println!("MEDIA_RECOVERED: auxiliares verificados; ninguna tarjeta modificada.");
        transfer?;
        anyhow::ensure!(copied && source_consumed, "La transferencia no se verificó físicamente");
        println!("MEDIA_OK: transferencia temporal y recuperación verificadas.");
        Ok(())
    })();
    if let Err(error) = result { println!("MEDIA_ERROR: {error:#}"); }
    true
}

#[cfg(test)] mod passive_history_tests {
    use super::*;
    fn fixture()->PathBuf {
        let root=std::env::temp_dir().join(format!("ws-passive-probe-{}",crate::flasher::generate_token()));
        let operation=root.join("abcdef0123456789abcd");fs::create_dir_all(&operation).unwrap();
        let mut books:HashMap<String,Option<Vec<u8>>>=crate::airlift::TRACKED_BOOKS_FILES.iter().map(|path|(path.to_string(),None)).collect();
        books.insert(crate::airlift::TRACKED_BOOKS_FILES[0].into(),Some(vec![1,2,3]));
        let journal=ProbeJournal{version:1,token:"abcdef0123456789abcd".into(),udid:"synthetic-device".into(),books};
        fs::write(operation.join("books-preimage.json"),serde_json::to_vec(&journal).unwrap()).unwrap();
        fs::write(operation.join("recovered.txt"),COMPLETED_PROBE_MARKER).unwrap();root
    }
    fn snapshot(root:&std::path::Path)->std::collections::BTreeMap<PathBuf,Vec<u8>> {
        let mut out=std::collections::BTreeMap::new();
        for operation in fs::read_dir(root).unwrap() {for entry in fs::read_dir(operation.unwrap().path()).unwrap() {let path=entry.unwrap().path();out.insert(path.strip_prefix(root).unwrap().into(),fs::read(path).unwrap());}}
        out
    }
    #[test] fn passive_completed_history_is_read_only_and_does_not_claim_historical_payload_authenticity() {
        let root=fixture();let before=snapshot(&root);assert_eq!(passive_completed_probe_count_at(&root).unwrap(),1);assert_eq!(snapshot(&root),before);
        let path=root.join("abcdef0123456789abcd/books-preimage.json");let mut value:serde_json::Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        value["books"][crate::airlift::TRACKED_BOOKS_FILES[0]][0]=serde_json::json!(4);fs::write(&path,serde_json::to_vec(&value).unwrap()).unwrap();
        // The legacy format has no independent historical hash. A plausible
        // changed JSON payload remains passive evidence, never OriginalVerified.
        let changed=snapshot(&root);assert_eq!(passive_completed_probe_count_at(&root).unwrap(),1);assert_eq!(snapshot(&root),changed);
        fs::remove_dir_all(root).unwrap();
    }
    #[test] fn passive_probe_missing_adulterated_extra_and_oversize_records_stay_denied() {
        for case in ["missing","marker","token","identity","field","books","byte","extra","oversize"] {
            let root=fixture();let operation=root.join("abcdef0123456789abcd");let path=operation.join("books-preimage.json");
            let mut value:serde_json::Value=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            match case {
                "missing"=>fs::remove_file(operation.join("recovered.txt")).unwrap(),
                "marker"=>fs::write(operation.join("recovered.txt"),b"unverified completion").unwrap(),
                "token"=>{value["token"]=serde_json::json!("00000000000000000000");fs::write(&path,serde_json::to_vec(&value).unwrap()).unwrap();},
                "identity"=>{value["udid"]=serde_json::json!("../foreign-device");fs::write(&path,serde_json::to_vec(&value).unwrap()).unwrap();},
                "field"=>{value["unknown"]=serde_json::json!(true);fs::write(&path,serde_json::to_vec(&value).unwrap()).unwrap();},
                "books"=>{value["books"]["Books/unknown.bin"]=serde_json::json!(null);fs::write(&path,serde_json::to_vec(&value).unwrap()).unwrap();},
                "byte"=>{value["books"][crate::airlift::TRACKED_BOOKS_FILES[0]][0]=serde_json::json!(256);fs::write(&path,serde_json::to_vec(&value).unwrap()).unwrap();},
                "extra"=>fs::write(operation.join("unknown.bin"),b"retained").unwrap(),
                _=>fs::write(&path,vec![b' ';MAX_PASSIVE_PROBE_JOURNAL as usize+1]).unwrap(),
            }
            let before=snapshot(&root);assert!(passive_completed_probe_count_at(&root).is_err(),"unexpected passive acceptance: {case}");assert_eq!(snapshot(&root),before);fs::remove_dir_all(root).unwrap();
        }
    }
    #[cfg(windows)] #[test] fn passive_probe_rejects_a_junction_without_reading_its_target_as_evidence() {
        use std::os::windows::process::CommandExt;
        let target=fixture();let parent=std::env::temp_dir().join(format!("ws-passive-link-{}",crate::flasher::generate_token()));fs::create_dir(&parent).unwrap();let link=parent.join("link");
        let status=std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","New-Item -ItemType Junction -Path $env:WALLET_STUDIO_PROBE_LINK -Value $env:WALLET_STUDIO_PROBE_TARGET -ErrorAction Stop | Out-Null"])
            .env("WALLET_STUDIO_PROBE_LINK",&link).env("WALLET_STUDIO_PROBE_TARGET",&target).creation_flags(0x08000000).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().unwrap();
        assert!(status.success(),"synthetic junction creation failed");let before=snapshot(&target);assert!(passive_completed_probe_count_at(&link).is_err());assert_eq!(snapshot(&target),before);
        fs::remove_dir(&link).unwrap();fs::remove_dir_all(parent).unwrap();fs::remove_dir_all(target).unwrap();
    }
}
