#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod afc;
mod animated_input;
mod animated_artwork;
mod design_library;
mod crop_editor;
mod update_info;
mod airlift;
mod airtraffic;
mod card_gallery;
mod app;
mod apple;
mod device;
mod flasher;
mod image_skin;
mod i18n;
mod passthm;
mod scanner;
mod wallet_backup;
mod wallet_export;
mod bank_artwork;
mod bank_backup;
mod bank_changes;
mod bank_runtime;
mod transport_probe;

fn main() -> eframe::Result<()> {
    if let Some(result) = run_prepare_animation_if_requested() {
        if let Err(error) = result {
            eprintln!("APNG_PREPARE_ERROR: {error:#}");
            std::process::exit(2);
        }
        return Ok(());
    }
    if airtraffic::run_worker_if_requested() {
        return Ok(());
    }
    if airtraffic::run_diagnostic_if_requested() {
        return Ok(());
    }
    if transport_probe::run_if_requested() {
        return Ok(());
    }
    if let Some(result) = run_capture_if_requested() {
        if let Err(error) = result {
            eprintln!("CAPTURA_ERROR: {error:#}");
            std::process::exit(2);
        }
        return Ok(());
    }
    let window_icon = image::load_from_memory(include_bytes!("../assets/wallet-studio.png"))
        .expect("bundled Wallet Studio icon is a valid image")
        .to_rgba8();
    let (icon_width, icon_height) = window_icon.dimensions();
    let icon_data = egui::IconData {
        rgba: window_icon.into_raw(),
        width: icon_width,
        height: icon_height,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("Wallet Studio")
            .with_icon(std::sync::Arc::new(icon_data)),
        ..Default::default()
    };

    eframe::run_native(
        "Wallet Studio",
        options,
        Box::new(|cc| Ok(Box::new(app::AirCardApp::new(cc)))),
    )
}

/// Offline prototype generation runs before any device query or Apple runtime.
fn run_prepare_animation_if_requested() -> Option<anyhow::Result<()>> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("--wallet-studio-prepare-animation") { return None; }
    Some((|| -> anyhow::Result<()> {
        use std::io::Write;
        let input = args.next().ok_or_else(|| anyhow::anyhow!("Falta el GIF de entrada"))?;
        let output = args.next().ok_or_else(|| anyhow::anyhow!("Falta la carpeta de salida"))?;
        anyhow::ensure!(args.next().is_none(), "Argumentos adicionales no permitidos");
        let artwork = animated_artwork::from_gif(std::path::Path::new(&input))?;
        let directory = std::path::PathBuf::from(output);
        std::fs::create_dir_all(&directory)?;
        let metadata = std::fs::symlink_metadata(&directory)?;
        anyhow::ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(), "La salida debe ser una carpeta normal");
        let outputs = [("cardBackgroundCombined@3x.png", &artwork.png_3x),
            ("cardBackgroundCombined@2x.png", &artwork.png_2x)];
        for (leaf, _) in outputs {
            match std::fs::symlink_metadata(directory.join(leaf)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                _ => anyhow::bail!("La salida {leaf} ya existe o no se pudo comprobar; no se sustituirá ningún archivo"),
            }
        }
        for (leaf, bytes) in outputs {
            let path = directory.join(leaf);
            let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;
            file.write_all(bytes)?; file.sync_all()?; drop(file);
            anyhow::ensure!(std::fs::read(&path)? == *bytes, "La salida {leaf} no pasó la lectura posterior");
        }
        println!("APNG_PREPARED_OFFLINE: {} fotogramas; @3x {} bytes; @2x {} bytes. No se consultó ni escribió ningún iPhone. La reproducción en Wallet no está comprobada.",
            artwork.frames, artwork.png_3x.len(), artwork.png_2x.len());
        Ok(())
    })())
}

/// Diagnostic and recovery commands use the same guarded device backend.
fn run_capture_if_requested() -> Option<anyhow::Result<()>> {
    let mut args = std::env::args().skip(1);
    let command = args.next();
    let bank_manifest = command.as_deref() == Some("--wallet-studio-capture-bank-manifest");
    let bank_inspection = command.as_deref() == Some("--wallet-studio-inspect-bank");
    let bank_state = command.as_deref() == Some("--wallet-studio-capture-bank-state");
    let cache_restore = command.as_deref() == Some("--wallet-studio-restore-cache");
    let cache_inspection = command.as_deref() == Some("--wallet-studio-inspect-cache");
    let cache_probe = command.as_deref() == Some("--wallet-studio-probe-cache");
    let test_bank_apply = command.as_deref() == Some("--wallet-studio-test-bank-apply");
    let test_bank_replace = command.as_deref() == Some("--wallet-studio-test-bank-replace");
    let test_bank_animation = command.as_deref() == Some("--wallet-studio-test-bank-animation");
    let test_bank_restore = command.as_deref() == Some("--wallet-studio-test-bank-restore");
    if !bank_manifest && !bank_inspection && !bank_state && !cache_restore && !cache_inspection && !cache_probe
        && !test_bank_apply && !test_bank_replace && !test_bank_restore && !test_bank_animation
        && command.as_deref() != Some("--wallet-studio-capture-original") { return None; }
    let result = (|| -> anyhow::Result<()> {
        let identifier = args.next().ok_or_else(|| anyhow::anyhow!("Falta la referencia de tarjeta o token de caché"))?;
        let test_image = if test_bank_apply || test_bank_replace || test_bank_animation {
            Some(args.next().ok_or_else(|| anyhow::anyhow!("Falta la imagen PNG de prueba"))?)
        } else { None };
        let cache_target = if cache_inspection || cache_probe {
            Some((args.next().ok_or_else(|| anyhow::anyhow!("Falta el sufijo de caché"))?,
                args.next().ok_or_else(|| anyhow::anyhow!("Falta el archivo de caché"))?))
        } else { None };
        anyhow::ensure!(args.next().is_none(), "Argumentos adicionales no permitidos");
        if !cache_restore {
            anyhow::ensure!(scanner::is_valid_card_hash(&identifier)
                && !identifier.contains('/') && !identifier.contains('\\')
                && identifier.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"+_-=".contains(&byte)),
                "Referencia de tarjeta inválida");
        }
        let devices = device::list_connected_devices()?;
        let devices: Vec<_> = devices.iter().filter(|item| item.has_transport(device::DeviceTransport::Usb)).collect();
        anyhow::ensure!(devices.len() == 1, "Conecta un único iPhone por USB");
        let phone = devices[0];
        if let Some(expected) = std::env::var_os("WALLET_STUDIO_EXPECT_DEVICE") {
            anyhow::ensure!(expected.to_str().is_some_and(|value| value == phone.udid),
                "El iPhone conectado no coincide con el dispositivo autorizado. No se inició la operación");
        }
        device::ensure_wallet_version_supported(&phone.udid)?;
        println!("Dispositivo: {} · iOS {}", phone.product_type, phone.ios_version);
        if cache_restore {
            wallet_export::restore_cache_leaf_by_token(&identifier, &phone.udid, device::ConnectionMode::Usb,
                |message| println!("{message}"))?;
            println!("CACHE_RESTORE_OK: bytes originales devueltos y verificados.");
            return Ok(());
        }
        if let Some(image_path) = test_image {
            if test_bank_animation {
                let safe_text = |message: &str| message.replace(&identifier, "[tarjeta protegida]")
                    .replace(&phone.udid, "[dispositivo protegido]");
                let artwork = animated_artwork::from_gif(std::path::Path::new(&image_path))
                    .map_err(|error| anyhow::anyhow!("No se pudo preparar el GIF experimental: {}", safe_text(&format!("{error:#}"))))?;
                println!("APNG_PREPARED: {} fotogramas conservados en @3x y @2x; no se enviará PDF.", artwork.frames);
                flasher::replace_bank_animation(&phone.udid, device::ConnectionMode::Usb, &identifier,
                    &artwork.png_3x, &artwork.png_2x,
                    |step, total, message| println!("ANIMATION_STEP: {step}/{total} {}", safe_text(message)),
                    |message| println!("{}", safe_text(message)))
                    .map_err(|error| anyhow::anyhow!("{}", safe_text(&format!("{error:#}"))))?;
                println!("BANK_ANIMATION_BYTES_VERIFIED: APNG y ausencia de PDF comprobados. La reproducción animada en Wallet requiere confirmación visual humana en el iPhone.");
                return Ok(());
            }
            let image = image::open(&image_path)?;
            let skin = image_skin::PreparedSkin::from_image_with_focus(image, 0.5, 0.5)?;
            if test_bank_replace {
                anyhow::ensure!(bank_backup::baseline_exists(&phone.udid, &identifier),
                    "No hay copia original verificable para este iPhone y tarjeta");
                let active = bank_changes::load_latest_intent(&phone.udid, &identifier)?
                    .is_some_and(|intent| intent.phase == "applied");
                anyhow::ensure!(active,
                    "La sustitución de prueba requiere un diseño previo confirmado y activo");
                let safe_text = |message: &str| message.replace(&identifier, "[tarjeta protegida]")
                    .replace(&phone.udid, "[dispositivo protegido]");
                flasher::replace_wallet_skin(&phone.udid, device::ConnectionMode::Usb, &identifier,
                    &skin.png, &skin.pdf,
                    |step, total, message| println!("REPLACE_STEP: {step}/{total} {}", safe_text(message)),
                    |message| println!("{}", safe_text(message)))?;
                println!("BANK_TEST_REPLACE_OK: diseño anterior restaurado y nuevo diseño verificado.");
                return Ok(());
            }
            flasher::flash_wallet_skin(&phone.udid, device::ConnectionMode::Usb, &identifier,
                &skin.png, &skin.pdf,
                |step, total, message| println!("APPLY_STEP: {step}/{total} {message}"),
                |message| println!("{message}"))?;
            println!("BANK_TEST_APPLY_OK: diseño de prueba enviado y estado de archivos/caché comprobado.");
            return Ok(());
        }
        if test_bank_restore {
            flasher::restore_wallet_original(&phone.udid, device::ConnectionMode::Usb, &identifier,
                |step, total, message| println!("RESTORE_STEP: {step}/{total} {message}"),
                |message| println!("{message}"))?;
            println!("BANK_TEST_RESTORE_OK: originales devueltos y estado de archivos/caché comprobado.");
            return Ok(());
        }
        if let Some((suffix, leaf)) = cache_target {
            if cache_probe {
                let token = wallet_export::remove_verified_cache_leaf(&phone.udid,
                    device::ConnectionMode::Usb, &identifier, &suffix, &leaf,
                    |message| println!("{message}"))?;
                if let Some(token) = token {
                    println!("CACHE_MOVE_OK: original guardado; iniciando devolución.");
                    wallet_export::restore_cache_leaf_by_token(&token, &phone.udid,
                        device::ConnectionMode::Usb, |message| println!("{message}"))?;
                    println!("CACHE_PROBE_OK: archivo movido y devuelto con lectura protegida idéntica.");
                } else {
                    println!("CACHE_PROBE_ALREADY_ABSENT: no se movió ningún archivo.");
                }
                return Ok(());
            }
            let state = wallet_export::inspect_cache_leaf(&phone.udid, device::ConnectionMode::Usb,
                &identifier, &suffix, &leaf, |message| println!("{message}"))?;
            println!("CACHE_STATUS: {suffix}/{leaf} = {state:?}");
            return Ok(());
        }
        if bank_state {
            bank_runtime::capture_baseline(&phone.udid, device::ConnectionMode::Usb, &identifier,
                |message| println!("{message}"))?;
            println!("BANK_STATE_CAPTURE_OK: estado bancario completo verificado.");
            return Ok(());
        }
        if bank_inspection {
            let manifest = wallet_export::capture_verified_original_asset(&phone.udid, device::ConnectionMode::Usb,
                &identifier, wallet_export::BANK_MANIFEST, |message| println!("{message}"))?;
            let states = wallet_export::inspect_bank_overrides(&phone.udid, device::ConnectionMode::Usb,
                &identifier, &manifest, |message| println!("{message}"))?;
            println!("BANK_INSPECTION_OK: {} estados locales comprobados", states.len());
            return Ok(());
        }
        if bank_manifest {
            let bytes = wallet_export::capture_verified_original_asset(&phone.udid,
                device::ConnectionMode::Usb, &identifier, wallet_export::BANK_MANIFEST,
                |message| println!("{message}"))?;
            println!("MANIFEST_RETURN_OK: original devuelto y lectura posterior idéntica.");
            let entries = bank_artwork::parse_manifest(&bytes)?;
            for entry in entries {
                let image = bank_artwork::download_verified_artwork(&entry)?;
                println!("ARTWORK_DOWNLOAD_OK: {} bytes, tamaño y SHA-1 verificados", image.len());
                break;
            }
            return Ok(());
        }
        let path = wallet_backup::capture_original_card(&phone.udid, device::ConnectionMode::Usb, &identifier,
            |message| println!("{message}"))?;
        anyhow::ensure!(path.is_some() && wallet_backup::backup_exists(&phone.udid, &identifier),
            "No hay copia completa verificada");
        println!("CAPTURA_OK: tres originales guardados; devolución verificada por el motor de captura.");
        Ok(())
    })();
    Some(result)
}
