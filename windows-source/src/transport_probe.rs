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
        fs::write(operation.join("recovered.txt"), b"Books verified; owned Media files absent; no card file touched.\n")?;
        println!("MEDIA_RECOVERED: auxiliares verificados; ninguna tarjeta modificada.");
        transfer?;
        anyhow::ensure!(copied && source_consumed, "La transferencia no se verificó físicamente");
        println!("MEDIA_OK: transferencia temporal y recuperación verificadas.");
        Ok(())
    })();
    if let Err(error) = result { println!("MEDIA_ERROR: {error:#}"); }
    true
}
