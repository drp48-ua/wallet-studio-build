use std::thread::sleep;
use std::time::Duration;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use crate::apple::{ATHostConnectionRef, get_apple_libraries};
use crate::device::{DeviceTransport, ensure_transport_available};

// Handshake adapted from dhava-gautama/AirCard-Windows e0eadb0 (MIT).
/// Replayable 84-byte host Grappa blob from yinyajiang/go-tunes.
/// Windows AirTrafficHost cannot mint a live FairPlay Grappa; sending
/// SendSyncRequest without this blob yields ErrorCode 12, and the DLL's
/// uninitialized blob yields ErrorCode 4 (invalid Grappa).
const HOST_GRAPPA: [u8; 84] = [
    0x01, 0x01, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x11,
    0x11, 0x11, 0x04, 0x40, 0xbc, 0x27, 0x85, 0xe0, 0xdb, 0xf1, 0x66, 0x36, 0x1e, 0x07, 0x98, 0x0a,
    0x5e, 0xa4, 0x8d, 0xba, 0x95, 0xb3, 0xb8, 0xea, 0x26, 0x5d, 0x62, 0xae, 0xfe, 0xa5, 0x1b, 0xb7,
    0xb1, 0x90, 0xe0, 0xb7, 0x71, 0x26, 0x29, 0x0a, 0xd3, 0x9b, 0xb1, 0x3f, 0xec, 0xc0, 0x8c, 0x25,
    0xa9, 0x56, 0x1c, 0x51, 0x7a, 0xc1, 0x1e, 0x64, 0x90, 0x5d, 0xa0, 0x29, 0xe6, 0x1b, 0xdf, 0xd0,
    0xba, 0x22, 0xc3, 0x13,
];

const LIBRARY_ID: &str = "12.6.0.100";
const HOST_VERSION: &str = "12.6.0.100";

fn plist_to_cf(libs: &crate::apple::AppleLibraries, value: &plist::Value) -> Result<crate::apple::CFTypeGuard> {
    let mut bytes = Vec::new();
    plist::to_writer_binary(&mut bytes, value).context("Failed to encode CF plist")?;
    libs.create_cf_plist_from_bytes(&bytes)
}

#[derive(serde::Serialize, serde::Deserialize)]
enum WorkerEvent {
    Log(String),
    Done(std::result::Result<(), String>),
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SyncRequest {
    udid: String,
    wifi: bool,
    assets: Vec<(String, String)>,
}

/// Native Apple reads may block indefinitely. Keep them in a child process so
/// timeout always stops and reaps the transport before the caller cleans staging.
pub fn run_worker_if_requested() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--wallet-studio-sync-worker") {
        return false;
    }
    let send = |event: WorkerEvent| {
        if let Ok(line) = serde_json::to_string(&event) {
            let mut stdout = std::io::stdout().lock();
            let _ = writeln!(stdout, "{line}");
            let _ = stdout.flush();
        }
    };
    let result = (|| -> Result<()> {
        let mut input = String::new();
        std::io::stdin().take(1_048_576).read_to_string(&mut input)?;
        let request: SyncRequest = serde_json::from_str(&input)?;
        anyhow::ensure!(request.assets.len() <= 128,
            "Solicitud de sincronización inválida");
        let assets: Vec<(&str, &str)> = request.assets.iter()
            .map(|(ident, dest)| (ident.as_str(), dest.as_str())).collect();
        sync_assets_via_airtraffic_internal(&request.udid,
            if request.wifi { DeviceTransport::Wifi } else { DeviceTransport::Usb },
            &assets, |message| send(WorkerEvent::Log(message.to_string())))
    })();
    send(WorkerEvent::Done(result.map_err(|error| format!("{error:#}"))));
    true
}

fn sync_timeout(_transport: DeviceTransport, assets: usize) -> Duration {
    // The Windows library handshake may need up to 90s for ReadyForSync.
    Duration::from_secs(120u64.max(assets as u64 * 2))
}

pub fn run_diagnostic_if_requested() -> bool {
    if std::env::args().nth(1).as_deref() != Some("--wallet-studio-diagnose-airtraffic") {
        return false;
    }
    let result = (|| -> Result<()> {
        println!("{}", crate::apple::verify_support()?);
        let devices = crate::device::list_connected_devices()?;
        let devices: Vec<_> = devices.iter().filter(|device| device.has_transport(DeviceTransport::Usb)).collect();
        anyhow::ensure!(devices.len() == 1, "Conecta un único iPhone por USB y desbloquéalo");
        println!("Dispositivo: {} · iOS {}", devices[0].product_type, devices[0].ios_version);
        if let Some(warning) = crate::device::wallet_version_warning(&devices[0].ios_version) {
            println!("Compatibilidad Wallet sin validar: {warning}");
        }
        println!("Diagnóstico de negociación; sin archivos ni escritura de tarjetas.");
        sync_assets_via_airtraffic(&devices[0].udid, DeviceTransport::Usb, &[], |message| println!("{message}"))
    })();
    match result {
        Ok(()) => println!("DIAGNOSTICO_OK: ReadyForSync confirmado; no demuestra permiso de lectura ni restauración."),
        Err(error) => println!("DIAGNOSTICO_ERROR: {error:#}"),
    }
    true
}

pub fn sync_assets_via_airtraffic<L>(
    udid: &str,
    transport: DeviceTransport,
    assets: &[(&str, &str)],
    mut log: L,
) -> Result<()>
where
    L: FnMut(&str),
{
    let request = SyncRequest {
        udid: udid.to_string(), wifi: transport == DeviceTransport::Wifi,
        assets: assets.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(),
    };
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--wallet-studio-sync-worker").stdin(Stdio::piped())
        .stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().context("No se pudo iniciar la sincronización aislada")?;
    let input_result = (|| -> Result<()> {
        let mut stdin = child.stdin.take().context("Canal de sincronización no disponible")?;
        stdin.write_all(&serde_json::to_vec(&request)?)?;
        Ok(())
    })();
    if let Err(error) = input_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    supervise_child(child, sync_timeout(transport, assets.len()), &mut log)
}

fn supervise_child<L>(mut child: std::process::Child, timeout: Duration, log: &mut L) -> Result<()>
where L: FnMut(&str) {
    let stdout = child.stdout.take().context("Falta el canal del proceso de sincronización")?;
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(event) = serde_json::from_str::<WorkerEvent>(&line) {
                if tx.send(event).is_err() { break; }
            }
        }
    });
    let start = std::time::Instant::now();
    let mut phase = "inicio de conexión".to_string();
    let result = loop {
        let remaining = timeout.saturating_sub(start.elapsed());
        match rx.recv_timeout(remaining) {
            Ok(WorkerEvent::Log(message)) => { phase = message.clone(); log(&message); }
            Ok(WorkerEvent::Done(result)) => break result.map_err(anyhow::Error::msg),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break Err(anyhow::anyhow!(
                "AirTraffic sync timed out ({}s). Sin respuesta del iPhone. Última fase: {}. Desbloquea el iPhone, abre Libros una vez y cierra iTunes/Dispositivos Apple. No se confirma la escritura.", timeout.as_secs(), phase)),
            Err(_) => break Err(anyhow::anyhow!("La sincronización terminó sin confirmar el resultado. Última fase: {}", phase)),
        }
    };
    // Even after Done, ensure the process cannot retain the native connection.
    let _ = child.kill();
    child.wait().context("No se pudo cerrar el proceso de sincronización; no continuar")?;
    let _ = reader.join();
    result
}

fn sync_assets_via_airtraffic_internal<L>(
    udid: &str,
    transport: DeviceTransport,
    assets: &[(&str, &str)],
    mut log: L,
) -> Result<()>
where
    L: FnMut(&str),
{
    ensure_transport_available(udid, transport)
        .context("Selected device transport disappeared before AirTraffic sync")?;
    log(&format!(
        "Connecting to iOS AirTraffic service (com.apple.atc) over {}...",
        transport.label()
    ));
    let libs = get_apple_libraries()?;
    let cf_udid = libs.create_cf_string(udid)?;
    let cf_library = libs.create_cf_string(LIBRARY_ID)?;

    // Windows ATH requires CreateWithLibrary(libraryID, udid, flags).
    // One-arg Create(udid) connects but cannot complete Grappa.
    let mut conn: ATHostConnectionRef =
        unsafe { (libs.at_host_connection_create_with_library)(cf_library.raw, cf_udid.raw, 0) };
    if conn.is_null() {
        log("CreateWithLibrary returned null, falling back to ATHostConnectionCreate...");
        conn = unsafe { (libs.at_host_connection_create)(cf_udid.raw) };
    }
    if conn.is_null() {
        bail!("ATHostConnectionCreateWithLibrary failed for the selected device");
    }

    let mut run_sync = || -> Result<()> {
        let session = unsafe { (libs.at_host_connection_get_current_session_number)(conn) };
        log(&format!("AirTraffic session {session}, sending HostInfo..."));

        let host_info_value = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("LibraryID".into(), LIBRARY_ID.into());
            d.insert("SyncHostName".into(), "airlift".into());
            d.insert("Version".into(), HOST_VERSION.into());
            d.insert(
                "SyncedDataclasses".into(),
                plist::Value::Array(vec!["Book".into()]),
            );
            d
        });
        let cf_host_info = plist_to_cf(&libs, &host_info_value)?;
        unsafe {
            (libs.at_host_connection_send_host_info)(conn, cf_host_info.raw);
        }

        log("Waiting for SyncAllowed from iPhone (keep screen unlocked)...");
        let mut sync_allowed = false;
        for _ in 0..20 {
            let msg = unsafe { (libs.at_host_connection_read_message)(conn) };
            if msg.is_null() {
                sleep(Duration::from_millis(150));
                continue;
            }
            let name_ref = unsafe { (libs.at_cf_message_get_name)(msg) };
            let name = libs.to_rust_string(name_ref);
            unsafe { (libs.cf_release)(msg) };
            if name == "SyncAllowed" {
                sync_allowed = true;
                break;
            } else {
                log(&format!("AirTraffic message: {name}"));
            }
        }
        if !sync_allowed {
            bail!("AirTraffic: SyncAllowed message not received. Ensure iPhone screen is unlocked and Books app is opened.");
        }

        log("SyncAllowed received! Sending RequestingSync with replay Grappa...");

        let requesting = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert(
                "Dataclasses".into(),
                plist::Value::Array(vec!["Book".into()]),
            );
            d.insert("DataclassAnchors".into(), {
                let mut a = plist::Dictionary::new();
                a.insert("Book".into(), "0".into());
                plist::Value::Dictionary(a)
            });
            d.insert("HostInfo".into(), {
                let mut hi = plist::Dictionary::new();
                hi.insert("Grappa".into(), plist::Value::Data(HOST_GRAPPA.to_vec()));
                hi.insert("LibraryID".into(), LIBRARY_ID.into());
                hi.insert("SyncHostName".into(), "airlift".into());
                hi.insert("Version".into(), HOST_VERSION.into());
                hi.insert(
                    "SyncedDataclasses".into(),
                    plist::Value::Array(vec!["Book".into()]),
                );
                plist::Value::Dictionary(hi)
            });
            d
        });
        let cf_params = plist_to_cf(&libs, &requesting)?;
        let cf_cmd = libs.create_cf_string("RequestingSync")?;
        let msg = unsafe { (libs.at_cf_message_create)(session, cf_cmd.raw, cf_params.raw) };
        if msg.is_null() {
            bail!("ATCFMessageCreate(RequestingSync) returned null");
        }
        let send_status = unsafe { (libs.at_host_connection_send_message)(conn, msg) };
        unsafe {
            (libs.cf_release)(msg);
        }
        log(&format!("RequestingSync send status={send_status}"));

        log("Waiting for ReadyForSync from iPhone...");
        let mut ready_for_sync = false;
        let ready_deadline = std::time::Instant::now() + Duration::from_secs(90);
        while std::time::Instant::now() < ready_deadline {
            let msg = unsafe { (libs.at_host_connection_read_message)(conn) };
            if msg.is_null() {
                sleep(Duration::from_millis(150));
                continue;
            }
            let name_ref = unsafe { (libs.at_cf_message_get_name)(msg) };
            let name = libs.to_rust_string(name_ref);
            log(&format!("AirTraffic message (post-RequestingSync): {name}"));
            if name == "ReadyForSync" {
                unsafe { (libs.cf_release)(msg) };
                ready_for_sync = true;
                break;
            }
            if name == "SyncFailed" || name == "SyncFinished" {
                // A single numeric parameter is enough for diagnostics. Never
                // print the full message, plist body, Grappa data or token.
                let error_code = libs.create_cf_string("ErrorCode").ok().and_then(|key| {
                    let param = unsafe { (libs.at_cf_message_get_param)(msg, key.raw) };
                    if param.is_null() {
                        return None;
                    }
                    let bytes = libs.cf_plist_to_bytes(param).ok()?;
                    let value = plist::Value::from_reader(std::io::Cursor::new(bytes)).ok()?;
                    numeric_error_code(&value)
                });
                unsafe { (libs.cf_release)(msg) };
                if let Some(code) = error_code {
                    bail!("AirTraffic returned {name} instead of ReadyForSync. ErrorCode={code}");
                }
                bail!("AirTraffic returned {name} instead of ReadyForSync. ErrorCode unavailable");
            }
            unsafe { (libs.cf_release)(msg) };
        }
        if !ready_for_sync {
            bail!("AirTraffic: ReadyForSync message not received from device");
        }

        if assets.is_empty() {
            log("ReadyForSync received (probe, no assets) — handshake OK.");
            return Ok(());
        }

        log("ReadyForSync received. Finishing Books metadata sync...");
        let cf_true = plist_to_cf(&libs, &plist::Value::Boolean(true))?;
        unsafe {
            (libs.at_host_connection_send_power_assertion)(conn, cf_true.raw);
        }

        let sync_types = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("Book".into(), plist::Value::Integer(1.into()));
            d
        });
        let cf_sync_types = plist_to_cf(&libs, &sync_types)?;
        let anchors = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("Book".into(), "0".into());
            d
        });
        let cf_anchors = plist_to_cf(&libs, &anchors)?;

        unsafe {
            (libs.at_host_connection_send_metadata_sync_finished)(
                conn,
                cf_sync_types.raw,
                cf_anchors.raw,
            );
        }

        // 6. Read AssetManifest
        let cf_key_manifest = libs.create_cf_string("AssetManifest")?;
        let mut manifest_val: Option<plist::Value> = None;

        for _ in 0..30 {
            let msg = unsafe { (libs.at_host_connection_read_message)(conn) };
            if msg.is_null() {
                sleep(Duration::from_millis(150));
                continue;
            }
            let name_ref = unsafe { (libs.at_cf_message_get_name)(msg) };
            let name = libs.to_rust_string(name_ref);
            if name == "AssetManifest" {
                let param = unsafe { (libs.at_cf_message_get_param)(msg, cf_key_manifest.raw) };
                if !param.is_null() {
                    if let Ok(bytes) = libs.cf_plist_to_bytes(param) {
                        manifest_val = plist::Value::from_reader(std::io::Cursor::new(bytes)).ok();
                    }
                }
                unsafe { (libs.cf_release)(msg) };
                break;
            } else if name == "SyncFailed" || name == "SyncFinished" {
                unsafe { (libs.cf_release)(msg) };
                bail!("AirTraffic returned unexpected terminating message: {}", name);
            }
            unsafe { (libs.cf_release)(msg) };
        }

        let Some(manifest) = manifest_val else {
            bail!("AirTraffic: AssetManifest was not received or failed to parse");
        };

        // Validate Book manifest contains downloads
        let book_entries = manifest
            .as_dictionary()
            .and_then(|d| d.get("Book"))
            .and_then(|v| v.as_array())
            .context("AssetManifest does not contain Book list")?;

        let mut available_downloads = Vec::new();
        for entry in book_entries {
            if let Some(dict) = entry.as_dictionary() {
                let is_dl = dict.get("IsDownload").and_then(|b| b.as_boolean()).unwrap_or(false);
                if is_dl {
                    if let Some(asset_id) = dict.get("AssetID").and_then(|s| s.as_string()) {
                        available_downloads.push(asset_id.to_string());
                    }
                }
            }
        }

        for (ident, _) in assets {
            if !available_downloads.iter().any(|d| d == ident) {
                bail!(
                    "Falta un archivo solicitado en el manifiesto de sincronización ({} entradas disponibles)",
                    available_downloads.len()
                );
            }
        }

        // 7. Dispatch AssetCompleted for each asset
        let cf_dataclass = libs.create_cf_string("Book")?;
        for (idx, (ident, dest)) in assets.iter().enumerate() {
            let cf_ident = libs.create_cf_string(ident)?;
            let cf_dest = libs.create_cf_string(dest)?;

            unsafe {
                (libs.at_host_connection_send_asset_completed)(
                    conn,
                    cf_ident.raw,
                    cf_dataclass.raw,
                    cf_dest.raw,
                );
            }

            if idx + 1 < assets.len() {
                if idx == 0 {
                    sleep(Duration::from_millis(400));
                } else {
                    sleep(Duration::from_millis(60));
                }
            }
        }

        log("Waiting for SyncFinished from iPhone...");
        loop {
            let message = unsafe { (libs.at_host_connection_read_message)(conn) };
            if message.is_null() { sleep(Duration::from_millis(150)); continue; }
            let name = libs.to_rust_string(unsafe { (libs.at_cf_message_get_name)(message) });
            unsafe { (libs.cf_release)(message) };
            if name == "SyncFinished" { return Ok(()); }
            if name == "SyncFailed" { bail!("El iPhone rechazó la sincronización (SyncFailed)"); }
            log(&format!("AirTraffic message: {}", name));
        }
    };

    let result = run_sync();
    unsafe {
        (libs.at_host_connection_release)(conn);
    }
    result
}

fn numeric_error_code(value: &plist::Value) -> Option<String> {
    value
        .as_signed_integer()
        .map(|n| n.to_string())
        .or_else(|| value.as_unsigned_integer().map(|n| n.to_string()))
        .or_else(|| {
            value.as_string().and_then(|s| {
                let t = s.trim();
                t.parse::<i64>().ok().map(|number| number.to_string())
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sync_deadline_accounts_for_transport_and_batch_size() {
        assert_eq!(sync_timeout(DeviceTransport::Usb, 3).as_secs(), 120);
        assert_eq!(sync_timeout(DeviceTransport::Wifi, 3).as_secs(), 120);
        assert_eq!(sync_timeout(DeviceTransport::Usb, 80).as_secs(), 160);
    }
    #[test]
    fn worker_request_preserves_transport_and_asset_pairs() {
        let input = SyncRequest { udid: "test-device".into(), wifi: true,
            assets: vec![("book-id".into(), "destination".into())] };
        let output: SyncRequest = serde_json::from_slice(&serde_json::to_vec(&input).unwrap()).unwrap();
        assert!(output.wifi);
        assert_eq!(output.assets, input.assets);
    }
    #[test]
    fn failure_details_exclude_non_numeric_private_parameters() {
        assert_eq!(numeric_error_code(&plist::Value::String("-".into())), None);
        assert_eq!(numeric_error_code(&plist::Value::String("private token".into())), None);
        assert_eq!(numeric_error_code(&plist::Value::Data(vec![1, 2])), None);
        assert_eq!(numeric_error_code(&plist::Value::Integer(4.into())), Some("4".into()));
    }
    #[test]
    fn stalled_worker_is_stopped_before_timeout_returns() {
        let mut command = Command::new("powershell.exe");
        command.args(["-NoProfile", "-NonInteractive", "-Command", "Start-Sleep -Seconds 30"])
            .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)] {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let child = command.spawn().unwrap();
        let start = std::time::Instant::now();
        let error = supervise_child(child, Duration::from_millis(100), &mut |_| {}).unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(5));
    }
}
