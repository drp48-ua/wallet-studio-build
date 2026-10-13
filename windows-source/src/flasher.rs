use std::thread::sleep;
use std::time::Duration;

use anyhow::{Context, Result, bail};

use crate::afc::AfcClient;
use crate::airlift::{
    LINK_PREFIX, RECOVERED_PREFIX, SOURCE_PREFIX, build_books_plist,
    build_streaming_zip_archive, build_streaming_zip_archive_multi, restore_books, snapshot_books,
    stage_streaming_zip,
};
use crate::airtraffic::sync_assets_via_airtraffic;
use crate::device::{ActiveDeviceSession, ConnectionMode};
use crate::wallet_backup::{capture_original_card, load_original_assets};

#[allow(dead_code)]
pub const TARGET_WALLET_ASSETS: &[&str] = &[
    "cardBackgroundCombined@3x.png",
    "cardBackgroundCombined@2x.png",
    "cardBackgroundCombined.pdf",
];

#[allow(dead_code)]
pub const CACHE_FILES: &[&str] = &[
    "FrontFace",
    "PlaceHolder",
    "Preview",
];

const CACHE_SUFFIXES: &[&str] = &[".cache", ".pkcache"];

fn cache_leaf_targets() -> Vec<(&'static str, &'static str)> {
    CACHE_SUFFIXES.iter().flat_map(|suffix| CACHE_FILES.iter().map(move |leaf| (*suffix, *leaf))).collect()
}

/// Cache invalidation is deliberately disabled until each cache move can be
/// journaled, returned exactly and verified on a supported device.
pub const BANK_APPLY_READY: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BankApplyPlan { Apply, RestoreThenApply, Block }

pub fn bank_apply_plan(phase: Option<&str>) -> BankApplyPlan {
    match phase {
        None | Some("restored") => BankApplyPlan::Apply,
        Some("applied" | "originals_verified") => BankApplyPlan::RestoreThenApply,
        Some(_) => BankApplyPlan::Block,
    }
}

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptGenRandom(
        hAlgorithm: *mut std::ffi::c_void,
        pbBuffer: *mut u8,
        cbBuffer: u32,
        dwFlags: u32,
    ) -> i32;
}

pub fn generate_token() -> String {
    let mut bytes = [0u8; 10];
    unsafe {
        let _ = BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            2, // BCRYPT_USE_SYSTEM_PREFERRED_RNG
        );
    }
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn preflight_wallet_write(udid: &str) -> Result<()> {
    crate::device::ensure_wallet_version_supported(udid)?;
    let pending = crate::wallet_export::pending_recovery_operations()?;
    anyhow::ensure!(pending.iter().all(|operation| !operation.udid.eq_ignore_ascii_case(udid)),
        "Hay una exportación de Wallet pendiente de recuperación para este iPhone; revisa esa operación antes de escribir.");
    Ok(())
}

fn validate_write_destination(target_dir: &str, leaf: &str, size: usize) -> Result<()> {
    anyhow::ensure!(target_dir.starts_with("/var/mobile/Library/")
        && !target_dir.contains(['\\', ':']) && !target_dir.chars().any(char::is_control)
        && target_dir.split('/').all(|part| part != ".." && part != "."),
        "Directorio de escritura no permitido");
    anyhow::ensure!(!leaf.is_empty() && leaf.len() <= 255 && leaf != "." && leaf != ".."
        && !leaf.contains(['/', '\\', ':']) && !leaf.chars().any(char::is_control),
        "Nombre de archivo de escritura no permitido");
    anyhow::ensure!(size <= 32 * 1024 * 1024, "Recurso de escritura demasiado grande");
    Ok(())
}

pub fn write_system_file<L>(
    udid: &str,
    connection_mode: ConnectionMode,
    target_dir: &str,
    leaf_name: &str,
    payload: &[u8],
    mut log: L,
) -> Result<()>
where
    L: FnMut(&str),
{
    validate_write_destination(target_dir, leaf_name, payload.len())?;
    let token = generate_token();
    let source = format!("{}{}", SOURCE_PREFIX, token);
    let link_dest = format!("{}{}", LINK_PREFIX, token);
    let recovered = format!("{}{}", RECOVERED_PREFIX, token);

    let link_ident = format!("../../{}/p0/p1/p2/link", source);
    let payload_ident = format!("../../{}/payload", source);
    let target_dest = format!("{}/{}", link_dest, leaf_name);

    let books_identifiers = vec![link_ident.clone(), payload_ident.clone()];
    let assets_to_sync = [
        (link_ident.as_str(), link_dest.as_str()),
        (payload_ident.as_str(), target_dest.as_str()),
    ];

    log(&format!("Connecting AFC for {}...", leaf_name));
    let session = ActiveDeviceSession::open(Some(udid), connection_mode)
        .context("Failed to open device session for writing")?;
    log(&format!("Connected over {}.", session.transport.label()));
    let afc = AfcClient::new(&session).context("Failed to open AFC connection")?;

    let snapshot = snapshot_books(&afc).context("Failed to snapshot Books state before staging")?;

    let archive_data = build_streaming_zip_archive(target_dir, payload)
        .context("Failed to build streaming zip archive")?;

    let books_plist = build_books_plist(&books_identifiers)
        .context("Failed to build Books.plist")?;

    let write_res = (|| -> Result<()> {
        log(&format!("Staging payload archive ({} bytes) via MobileInstallation...", archive_data.len()));
        stage_streaming_zip(&session, &source, &archive_data)
            .context("Failed to stage streaming zip conduit")?;

        let link_obj = format!("{}/p0/p1/p2/link", source);
        let payload_obj = format!("{}/payload", source);
        if !afc.exists(&source) || !afc.exists(&link_obj) || !afc.exists(&payload_obj) {
            bail!("StreamingZip completed but staging link/payload object missing on AFC");
        }

        afc.make_directory_recursive("Books/Sync")?;
        afc.write_file("Books/Sync/Books.plist", &books_plist)?;
        if !afc.exists("Books/Sync/Books.plist") {
            bail!("Failed to stage Books/Sync/Books.plist");
        }

        log(&format!("Synchronizing {} with AirTraffic host daemon...", leaf_name));
        sync_assets_via_airtraffic(udid, session.transport, &assets_to_sync, &mut log)
            .context("AirTraffic sync failed")?;

        Ok(())
    })();

    let _ = afc.remove_path(&link_dest);
    let _ = afc.remove_path(&recovered);
    let _ = afc.remove_tree(&source);
    sleep(Duration::from_millis(800));

    let restore_res = restore_books(&afc, &snapshot);

    finish_cleanup(write_res, restore_res)?;
    log(&format!("Successfully written: {}", leaf_name));

    Ok(())
}

pub fn write_system_files_batch<L>(
    udid: &str,
    connection_mode: ConnectionMode,
    target_dir: &str,
    items: &[(&str, &[u8])],
    mut log: L,
) -> Result<()>
where
    L: FnMut(&str),
{
    if items.is_empty() {
        return Ok(());
    }
    for (leaf, payload) in items {
        validate_write_destination(target_dir, leaf, payload.len())?;
    }
    if items.len() == 1 {
        return write_system_file(
            udid,
            connection_mode,
            target_dir,
            items[0].0,
            items[0].1,
            log,
        );
    }

    log(&format!("Packaging atomic batch of {} file(s) for {}...", items.len(), target_dir));

    let token = generate_token();
    let source = format!("{}{}", SOURCE_PREFIX, token);
    let link_dest = format!("{}{}", LINK_PREFIX, token);
    let recovered = format!("{}{}", RECOVERED_PREFIX, token);

    let link_ident = format!("../../{}/p0/p1/p2/link", source);
    let mut books_identifiers = Vec::with_capacity(items.len() + 1);
    books_identifiers.push(link_ident.clone());

    let mut assets_to_sync: Vec<(String, String)> = Vec::with_capacity(items.len() + 1);
    assets_to_sync.push((link_ident, link_dest.clone()));

    for (idx, (leaf, _)) in items.iter().enumerate() {
        let payload_ident = format!("../../{}/payload_{}", source, idx);
        let target_dest = format!("{}/{}", link_dest, leaf);
        books_identifiers.push(payload_ident.clone());
        assets_to_sync.push((payload_ident, target_dest));
    }

    log(&format!("Connecting AFC for batch of {} assets...", items.len()));
    let session = ActiveDeviceSession::open(Some(udid), connection_mode)
        .context("Failed to open device session for writing")?;
    log(&format!("Connected over {}.", session.transport.label()));
    let afc = AfcClient::new(&session).context("Failed to open AFC connection")?;

    let snapshot = snapshot_books(&afc).context("Failed to snapshot Books state before staging")?;

    let archive_data = build_streaming_zip_archive_multi(target_dir, items)
        .context("Failed to build multi-payload streaming zip archive")?;

    let books_plist = build_books_plist(&books_identifiers)
        .context("Failed to build Books.plist for batch")?;

    let write_res = (|| -> Result<()> {
        log(&format!("Staging multi-payload archive ({} bytes, {} files) via MobileInstallation...", archive_data.len(), items.len()));
        stage_streaming_zip(&session, &source, &archive_data)
            .context("Failed to stage streaming zip conduit")?;

        let link_obj = format!("{}/p0/p1/p2/link", source);
        let payload_obj = format!("{}/payload_0", source);
        let fallback_obj = format!("{}/payload", source);
        if !afc.exists(&source) || !afc.exists(&link_obj) || (!afc.exists(&payload_obj) && !afc.exists(&fallback_obj)) {
            bail!("StreamingZip completed but staging link/payload object missing on AFC");
        }

        afc.make_directory_recursive("Books/Sync")?;
        afc.write_file("Books/Sync/Books.plist", &books_plist)?;
        if !afc.exists("Books/Sync/Books.plist") {
            bail!("Failed to stage Books/Sync/Books.plist");
        }

        log(&format!("Synchronizing batch ({} items) with AirTraffic host daemon in single session...", items.len()));
        let assets_refs: Vec<(&str, &str)> = assets_to_sync.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        sync_assets_via_airtraffic(udid, session.transport, &assets_refs, &mut log)
            .context("AirTraffic batch sync failed")?;

        Ok(())
    })();

    let _ = afc.remove_path(&link_dest);
    let _ = afc.remove_path(&recovered);
    let _ = afc.remove_tree(&source);
    sleep(Duration::from_millis(800));

    let restore_res = restore_books(&afc, &snapshot);

    finish_cleanup(write_res, restore_res)?;
    log(&format!("Batch injection of {} file(s) completed successfully!", items.len()));

    Ok(())
}

pub fn flash_wallet_skin<F, L>(
    udid: &str,
    connection_mode: ConnectionMode,
    card_hash: &str,
    skin_png: &[u8],
    skin_pdf: &[u8],
    mut progress: F,
    mut log: L,
) -> Result<()>
where
    F: FnMut(usize, usize, &str),
    L: FnMut(&str),
{
    anyhow::ensure!(BANK_APPLY_READY,
        "La aplicación está bloqueada: falta una invalidación de caché con diario y restauración exacta. No se escribió nada.");
    preflight_wallet_write(udid)?;
    anyhow::ensure!(crate::wallet_export::is_exact_card_hash(card_hash),
        "Invalid Wallet card path identifier");

    if let Some(baseline) = crate::bank_backup::load_baseline(udid, card_hash)? {
        return flash_bank_wallet_skin(udid, connection_mode, card_hash, &baseline,
            skin_png, skin_pdf, &mut progress, &mut log);
    }

    log(&format!("Target Card Hash: {}", card_hash));
    log(&format!(
        "Skin payload size: {} bytes PNG, {} bytes PDF",
        skin_png.len(),
        skin_pdf.len()
    ));
    let resolved_hash = capture_original_card(udid, connection_mode, card_hash, &mut log)
        .context("Failed to inspect the original Wallet card face")?
        .unwrap_or_else(|| card_hash.to_string());
    // AFC may hide bank card originals. Never turn a failed capture into a write.
    load_original_assets(udid, card_hash).context(
        "No hay copia original recuperable. AFC restringe la lectura de esta tarjeta; la escritura queda bloqueada")?;
    let pkpass_dir = format!("/var/mobile/Library/Passes/Cards/{}.pkpass", resolved_hash);

    let total_steps = 3;
    progress(1, total_steps, "Writing card artwork assets (@3x, @2x, .pdf)...");
    log("[1/3] Writing card artwork assets (@3x.png, @2x.png, cardBackgroundCombined.pdf)...");

    let skin_2x = resize_wallet_artwork_2x(skin_png)?;
    let card_assets: [(&str, &[u8]); 3] = [
        ("cardBackgroundCombined@3x.png", skin_png),
        ("cardBackgroundCombined@2x.png", &skin_2x),
        ("cardBackgroundCombined.pdf", skin_pdf),
    ];

    write_system_files_batch(
        udid,
        connection_mode,
        &pkpass_dir,
        &card_assets,
        &mut log,
    ).context("No se confirmó la escritura. No se reintentará automáticamente")?;

    invalidate_wallet_caches(
        udid,
        connection_mode,
        resolved_hash.as_str(),
        &mut progress,
        &mut log,
    )?;

    progress(total_steps, total_steps, "Card skin updated successfully!");
    log("Card skin write finished! Close and reopen Wallet on iPhone to view.");
    Ok(())
}

/// Safely replace the currently applied design for this exact card.
/// When the previous write is verified as applied, restore the captured
/// baseline completely before creating the next write intent. Uncertain or
/// malformed prior state fails closed and keeps its evidence for recovery.
pub fn replace_wallet_skin<F, L>(
    udid: &str,
    connection_mode: ConnectionMode,
    card_hash: &str,
    skin_png: &[u8],
    skin_pdf: &[u8],
    mut progress: F,
    mut log: L,
) -> Result<()>
where
    F: FnMut(usize, usize, &str),
    L: FnMut(&str),
{
    if !crate::bank_backup::baseline_exists(udid, card_hash) {
        // Legacy non-bank passes retain the original hash-verified backup route.
        load_original_assets(udid, card_hash)
            .context("Falta una copia original verificable; no se puede sustituir el diseño")?;
        return flash_wallet_skin(udid, connection_mode, card_hash, skin_png, skin_pdf, progress, log);
    }
    let intent = crate::bank_changes::load_latest_intent(udid, card_hash)?;
    let plan = bank_apply_plan(intent.as_ref().map(|intent| intent.phase.as_str()));
    anyhow::ensure!(plan != BankApplyPlan::Block,
        "Hay un cambio anterior sin resolver; se bloquea la sustitución");
    let restore_first = plan == BankApplyPlan::RestoreThenApply;
    if restore_first {
        log("Restaurando la última imagen aplicada antes de sustituirla.");
        restore_wallet_original(udid, connection_mode, card_hash,
            |step, total, message| progress(if total == 1 { 3 } else { step.min(3) }, 6, message),
            |message| log(message))
            .context("No se pudo restaurar el diseño anterior; no se intentó aplicar el nuevo")?;
    }
    flash_wallet_skin(udid, connection_mode, card_hash, skin_png, skin_pdf,
        |step, total, message| progress((if total == 1 { 3 } else { step.min(3) }) + if restore_first { 3 } else { 0 },
            if restore_first { 6 } else { 3 }, message),
        |message| log(message))
}

pub fn restore_wallet_original<F, L>(
    udid: &str,
    connection_mode: ConnectionMode,
    card_hash: &str,
    mut progress: F,
    mut log: L,
) -> Result<()>
where
    F: FnMut(usize, usize, &str),
    L: FnMut(&str),
{
    preflight_wallet_write(udid)?;
    anyhow::ensure!(crate::scanner::is_valid_card_hash(card_hash),
        "El identificador de la tarjeta no es válido");
    if let Some(baseline) = crate::bank_backup::load_baseline(udid, card_hash)? {
        return restore_bank_wallet_original(udid, connection_mode, card_hash, &baseline,
            &mut progress, &mut log);
    }
    let original_assets = load_original_assets(udid, card_hash)
        .context("Could not load the original Wallet card face backup")?;
    let asset_refs: Vec<(&str, &[u8])> = original_assets
        .iter()
        .map(|(asset, data)| (asset.as_str(), data.as_slice()))
        .collect();

    log(&format!(
        "Restoring {} original card artwork asset(s) for hash {}...",
        asset_refs.len(),
        card_hash
    ));
    progress(1, 3, "Restoring original card artwork...");

    let pkpass_dir = format!("/var/mobile/Library/Passes/Cards/{}.pkpass", card_hash);
    write_system_files_batch(
        udid,
        connection_mode,
        &pkpass_dir,
        &asset_refs,
        &mut log,
    ).context("No se confirmó la restauración. No se reintentará automáticamente")?;

    invalidate_wallet_caches(
        udid,
        connection_mode,
        card_hash,
        &mut progress,
        &mut log,
    )?;

    progress(3, 3, "Original card face restored successfully!");
    log("Original card face restored. Close and reopen Wallet on iPhone to view it.");
    Ok(())
}

fn bank_baseline_payloads(baseline: &crate::bank_backup::Baseline) -> Result<Vec<(String, Vec<u8>)>> {
    let mut payloads = Vec::with_capacity(TARGET_WALLET_ASSETS.len());
    for leaf in TARGET_WALLET_ASSETS {
        match baseline.local_assets.get(*leaf) {
            Some(crate::bank_backup::LocalAssetState::Present { bytes }) =>
                payloads.push(((*leaf).to_string(), bytes.clone())),
            Some(crate::bank_backup::LocalAssetState::Absent) => {},
            None => bail!("El baseline bancario no contiene el estado de {leaf}"),
        }
    }
    Ok(payloads)
}

fn verify_written_assets<L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    expected: &[(String, Vec<u8>)], log: &mut L) -> Result<()>
where L: FnMut(&str) {
    for (leaf, expected_bytes) in expected {
        let readback = crate::wallet_export::capture_verified_original_asset(
            udid, mode, card_hash, leaf, &mut *log)
            .with_context(|| format!("No se pudo confirmar mediante lectura protegida el archivo {leaf}"))?;
        anyhow::ensure!(readback == *expected_bytes,
            "La lectura protegida de {leaf} difiere de los bytes enviados. Se conserva el diario para recuperación.");
    }
    Ok(())
}

fn original_needs_write(expected: &[u8], observed: Option<&[u8]>) -> bool {
    observed != Some(expected)
}

fn absent_restore_payload<'a>(intent: &'a crate::bank_changes::Intent, leaf: &str) -> Result<Option<&'a [u8]>> {
    if intent.absent_assets.iter().any(|absent| absent == leaf) {
        anyhow::ensure!(!intent.assets.contains_key(leaf), "El intento mezcla presencia y ausencia");
        Ok(None)
    } else {
        Ok(Some(intent.assets.get(leaf).context("El intento no guarda los bytes propios que deben retirarse")?.as_slice()))
    }
}

fn verify_absent_assets<L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    baseline: &crate::bank_backup::Baseline, absent: &[String], log: &mut L) -> Result<()>
where L: FnMut(&str) {
    if absent.is_empty() { return Ok(()); }
    anyhow::ensure!(absent == ["cardBackgroundCombined.pdf"], "Ausencia experimental no admitida");
    anyhow::ensure!(absent.iter().all(|leaf| matches!(baseline.local_assets.get(leaf),
        Some(crate::bank_backup::LocalAssetState::Absent))),
        "El experimento solo permite PDF originalmente ausente; no se retirará ningún original");
    let manifest = crate::wallet_export::capture_verified_original_asset(udid, mode, card_hash,
        crate::wallet_export::BANK_MANIFEST, &mut *log)?;
    anyhow::ensure!(manifest == baseline.manifest_bytes, "El manifiesto ya no coincide con el baseline exacto");
    let states = crate::wallet_export::inspect_bank_overrides(udid, mode, card_hash, &manifest, &mut *log)?;
    anyhow::ensure!(absent.iter().all(|leaf| matches!(states.get(leaf), Some(crate::wallet_export::MetadataState::Absent))),
        "No se demostró la ausencia protegida del PDF. Conserva el diario; no se eliminará contenido desconocido");
    Ok(())
}

/// CLI experiment only. Byte readback does not establish animated rendering in Wallet.
pub fn replace_bank_animation<F, L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    png_3x: &[u8], png_2x: &[u8], mut progress: F, mut log: L) -> Result<()>
where F: FnMut(usize, usize, &str), L: FnMut(&str) {
    anyhow::ensure!(BANK_APPLY_READY && crate::wallet_export::is_exact_card_hash(card_hash),
        "No se cumplen las condiciones de escritura experimental");
    crate::animated_artwork::validate_pair(png_3x, png_2x)?;
    let baseline = crate::bank_backup::load_baseline(udid, card_hash)?
        .context("El experimento requiere un baseline bancario exacto y verificable")?;
    anyhow::ensure!(matches!(baseline.local_assets.get("cardBackgroundCombined.pdf"),
        Some(crate::bank_backup::LocalAssetState::Absent)),
        "El PDF estaba presente o no tiene evidencia de ausencia original; experimento bloqueado");
    preflight_wallet_write(udid)?;
    let intent = crate::bank_changes::load_latest_intent(udid, card_hash)?;
    let plan = bank_apply_plan(intent.as_ref().map(|intent| intent.phase.as_str()));
    anyhow::ensure!(plan != BankApplyPlan::Block, "Hay un cambio incierto; restaura el original antes del experimento");
    if plan == BankApplyPlan::RestoreThenApply {
        restore_wallet_original(udid, mode, card_hash, &mut progress, &mut log)?;
    }
    let payloads = vec![("cardBackgroundCombined@3x.png".into(), png_3x.to_vec()),
        ("cardBackgroundCombined@2x.png".into(), png_2x.to_vec())];
    flash_bank_assets(udid, mode, card_hash, &baseline, payloads,
        vec!["cardBackgroundCombined.pdf".into()], &mut progress, &mut log)
}

fn flash_bank_wallet_skin<F, L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    baseline: &crate::bank_backup::Baseline, skin_png: &[u8], skin_pdf: &[u8],
    progress: &mut F, log: &mut L) -> Result<()>
where F: FnMut(usize, usize, &str), L: FnMut(&str) {
    let skin_2x = resize_wallet_artwork_2x(skin_png)?;
    let payloads = vec![
        ("cardBackgroundCombined@3x.png".to_string(), skin_png.to_vec()),
        ("cardBackgroundCombined@2x.png".to_string(), skin_2x),
        ("cardBackgroundCombined.pdf".to_string(), skin_pdf.to_vec()),
    ];
    flash_bank_assets(udid, mode, card_hash, baseline, payloads, Vec::new(), progress, log)
}

fn flash_bank_assets<F, L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    baseline: &crate::bank_backup::Baseline, payloads: Vec<(String, Vec<u8>)>,
    absent: Vec<String>, progress: &mut F, log: &mut L) -> Result<()>
where F: FnMut(usize, usize, &str), L: FnMut(&str) {
    anyhow::ensure!(baseline.local_assets.len() == TARGET_WALLET_ASSETS.len(),
        "El baseline no registra exactamente las tres rutas bancarias");
    // Before starting a new two-file intent, prove PDF absence through the same
    // protected metadata path used by restoration. Never infer it from AFC denial.
    verify_absent_assets(udid, mode, card_hash, baseline, &absent, log)?;
    let operation = crate::bank_changes::save_intent(udid, card_hash, &payloads)?;
    let refs: Vec<(&str, &[u8])> = payloads.iter().map(|(leaf, bytes)| (leaf.as_str(), bytes.as_slice())).collect();
    let target = format!("/var/mobile/Library/Passes/Cards/{card_hash}.pkpass");
    progress(1, 3, "Escribiendo diseño con diario de recuperación…");
    write_system_files_batch(udid, mode, &target, &refs, &mut *log)
        .context("La transferencia quedó incierta; se conserva el intento y no se reintenta")?;
    progress(2, 3, "Comprobando cada archivo mediante exportación protegida…");
    verify_written_assets(udid, mode, card_hash, &payloads, log)?;
    verify_absent_assets(udid, mode, card_hash, baseline, &absent, log)?;
    progress(3, 3, "Archivos verificados; comprobando caché de Wallet…");
    invalidate_wallet_caches(udid, mode, card_hash, progress, log)?;
    crate::bank_changes::mark_applied(&operation)
}

fn restore_bank_wallet_original<F, L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    baseline: &crate::bank_backup::Baseline, progress: &mut F, log: &mut L) -> Result<()>
where F: FnMut(usize, usize, &str), L: FnMut(&str) {
    let intent = crate::bank_changes::load_latest_intent(udid, card_hash)?
        .context("No hay un intento de escritura bancario que restaurar")?;
    anyhow::ensure!(intent.phase != "restored", "El intento bancario ya está restaurado");
    if intent.phase != "originals_verified" {
        let originals = bank_baseline_payloads(baseline)?;
        if !originals.is_empty() {
            let mut needs_write = Vec::new();
            for (leaf, expected) in &originals {
                match crate::wallet_export::capture_verified_original_asset(
                    udid, mode, card_hash, leaf, &mut *log) {
                    Ok(observed) if !original_needs_write(expected, Some(&observed)) => {
                        log(&format!("El original {leaf} ya coincide byte por byte; se omite la escritura."));
                    }
                    Ok(_) => needs_write.push((leaf.clone(), expected.clone())),
                    Err(_) => {
                        // An unreadable result is never treated as proof. Retry
                        // by returning the saved baseline, then require readback.
                        log(&format!("No se pudo confirmar {leaf} antes de restaurar; se devolverá el baseline guardado."));
                        needs_write.push((leaf.clone(), expected.clone()));
                    }
                }
            }
            if !needs_write.is_empty() {
                let refs: Vec<(&str, &[u8])> = needs_write.iter().map(|(leaf, bytes)| (leaf.as_str(), bytes.as_slice())).collect();
                let target = format!("/var/mobile/Library/Passes/Cards/{card_hash}.pkpass");
                progress(1, 3, "Devolviendo solo los bytes originales aún distintos…");
                write_system_files_batch(udid, mode, &target, &refs, &mut *log)
                    .context("La restauración quedó incierta; el baseline y el intento se conservan")?;
                progress(2, 3, "Comprobando cada original mediante lectura protegida…");
                verify_written_assets(udid, mode, card_hash, &needs_write, &mut *log)?;
            }
        }
        for leaf in TARGET_WALLET_ASSETS {
            if matches!(baseline.local_assets.get(*leaf), Some(crate::bank_backup::LocalAssetState::Absent)) {
                let Some(owned) = absent_restore_payload(&intent, leaf)? else {
                    verify_absent_assets(udid, mode, card_hash, baseline, &intent.absent_assets, log)?;
                    continue;
                };
                progress(2, 3, &format!("Retirando solo el override propio verificado: {leaf}…"));
                crate::wallet_export::remove_verified_override(udid, mode, card_hash, leaf, owned, &mut *log)
                    .with_context(|| format!("No se pudo demostrar la ausencia original de {leaf}; se conserva el intento"))?;
            }
        }
        crate::bank_changes::mark_originals_verified(&intent.operation_dir)
            .context("Se restauraron los originales, pero no se pudo guardar el checkpoint de verificación")?;
    }
    if intent.phase == "originals_verified" {
        // A retry after cache failure cannot silently accept a newly appeared PDF.
        verify_absent_assets(udid, mode, card_hash, baseline, &intent.absent_assets, log)?;
    }
    progress(3, 3, "Originales ya verificados; comprobando caché de Wallet…");
    invalidate_wallet_caches(udid, mode, card_hash, progress, log)?;
    crate::bank_changes::mark_restored(&intent.operation_dir)
}

fn resize_wallet_artwork_2x(png_3x: &[u8]) -> Result<Vec<u8>> {
    use image::{ImageFormat, imageops::FilterType};
    let image = image::load_from_memory(png_3x).context("Could not decode 3x wallet artwork")?;
    let resized = image.resize_exact(1_024, 646, FilterType::Lanczos3);
    let mut bytes = Vec::new();
    resized.write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .context("Could not encode 2x wallet artwork")?;
    Ok(bytes)
}

fn invalidate_wallet_caches<F, L>(
    udid: &str,
    connection_mode: ConnectionMode,
    card_hash: &str,
    progress: &mut F,
    log: &mut L,
) -> Result<()> where
    F: FnMut(usize, usize, &str),
    L: FnMut(&str),
{
    progress(1, 1, "Retirando cachés una a una con copia local y diario…");
    for (suffix, leaf) in cache_leaf_targets() {
        crate::wallet_export::remove_verified_cache_leaf(
            udid, connection_mode, card_hash, suffix, leaf, &mut *log)
            .with_context(|| format!("No se pudo verificar la retirada de caché {suffix}/{leaf}"))?;
    }
    log("Las seis hojas de caché se comprobaron individualmente; cada retirada conserva copia exacta y diario en el PC.");
    Ok(())
}

fn finish_cleanup(write: Result<()>, restore: Result<()>) -> Result<()> {
    match (write, restore) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(write), Ok(())) => Err(write),
        (Ok(()), Err(restore)) => Err(restore.context("No se pudo recuperar el estado auxiliar de Libros")),
        (Err(write), Err(restore)) => bail!("Sincronización fallida: {write:#}. Recuperación auxiliar de Libros fallida: {restore:#}"),
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    fn failed_cleanup_cannot_be_hidden_by_transfer_failure() {
        let error = finish_cleanup(Err(anyhow::anyhow!("transfer-test")),
            Err(anyhow::anyhow!("cleanup-test"))).unwrap_err().to_string();
        assert!(error.contains("transfer-test") && error.contains("cleanup-test"));
        assert!(finish_cleanup(Ok(()), Err(anyhow::anyhow!("cleanup-test"))).is_err());
    }
}

pub fn flash_passcode_theme<F, L>(
    udid: &str,
    connection_mode: ConnectionMode,
    items: &[(String, String, Vec<u8>)],
    mut progress: F,
    mut log: L,
) -> Result<()>
where
    F: FnMut(usize, usize, &str),
    L: FnMut(&str),
{
    preflight_wallet_write(udid)?;
    let total = items.len();
    log(&format!("Flashing passcode theme ({} assets)...", total));

    let mut dirs_map: std::collections::BTreeMap<String, Vec<(&str, &[u8])>> = std::collections::BTreeMap::new();
    for (tdir, leaf, payload) in items {
        dirs_map.entry(tdir.clone()).or_default().push((leaf.as_str(), payload.as_slice()));
    }

    let total_dirs = dirs_map.len();
    let mut dir_idx = 0;

    for (target_dir, dir_items) in &dirs_map {
        dir_idx += 1;
        let tdir_name = std::path::Path::new(target_dir)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(target_dir);

        progress(
            dir_idx,
            total_dirs,
            &format!("Flashing {} ({} assets in atomic batch)...", tdir_name, dir_items.len()),
        );
        log(&format!("Flashing batch of {} assets into {}...", dir_items.len(), tdir_name));

        write_system_files_batch(
            udid,
            connection_mode,
            target_dir,
            dir_items,
            &mut log,
        ).with_context(|| format!("No se reintentará el lote {}: el resultado puede ser incierto", tdir_name))?;
    }

    progress(total_dirs, total_dirs, "Passcode theme applied successfully!");
    log("Passcode theme successfully written! Lock or reboot iPhone to see new keypad.");
    Ok(())
}

#[cfg(test)]
mod wallet_artwork_tests {
    use super::*;

    #[test]
    fn restoration_plans_explicit_absence_without_guessing_missing_owned_payloads() {
        let mut intent = crate::bank_changes::Intent {
            operation_dir: std::path::PathBuf::new(), udid: "test-device".into(), card_hash: "test-card".into(),
            assets: std::collections::BTreeMap::new(), absent_assets: vec!["cardBackgroundCombined.pdf".into()], phase: "prepared".into(),
        };
        assert!(absent_restore_payload(&intent,"cardBackgroundCombined.pdf").unwrap().is_none());
        assert!(absent_restore_payload(&intent,"cardBackgroundCombined@3x.png").is_err());
        intent.assets.insert("cardBackgroundCombined.pdf".into(), b"owned bytes".to_vec());
        assert!(absent_restore_payload(&intent,"cardBackgroundCombined.pdf").is_err());
        intent.absent_assets.clear();
        assert_eq!(absent_restore_payload(&intent,"cardBackgroundCombined.pdf").unwrap(), Some(b"owned bytes".as_slice()));
    }

    #[test]
    fn write_destinations_reject_path_escape_before_device_access() {
        assert!(validate_write_destination("/var/mobile/Library/Passes/Cards/example.pkpass", "cardBackgroundCombined@3x.png", 20).is_ok());
        assert!(validate_write_destination("/var/mobile/Library/Caches/TelephonyUI-10", "_big", 0).is_ok());
        for leaf in ["../outside", "/absolute", "a/b", "a\\b", "a:stream", "..", "", "a\n"] {
            assert!(validate_write_destination("/var/mobile/Library/Caches/TelephonyUI-10", leaf, 1).is_err());
        }
        for dir in ["/var/mobile/Library/../../etc", "/etc", "relative", "/var/mobile/Library/Caches\n"] {
            assert!(validate_write_destination(dir, "image.png", 1).is_err());
        }
    }

    #[test]
    fn replacing_a_design_restores_only_confirmed_prior_states() {
        assert_eq!(bank_apply_plan(None), BankApplyPlan::Apply);
        assert_eq!(bank_apply_plan(Some("restored")), BankApplyPlan::Apply);
        assert_eq!(bank_apply_plan(Some("applied")), BankApplyPlan::RestoreThenApply);
        assert_eq!(bank_apply_plan(Some("originals_verified")), BankApplyPlan::RestoreThenApply);
        assert_eq!(bank_apply_plan(Some("prepared")), BankApplyPlan::Block);
        assert_eq!(bank_apply_plan(Some("corrupt")), BankApplyPlan::Block);
    }
    #[test]
    fn generated_2x_artwork_has_expected_dimensions() {
        let mut input = Vec::new();
        image::DynamicImage::new_rgb8(1_536, 969).write_to(&mut std::io::Cursor::new(&mut input), image::ImageFormat::Png).unwrap();
        let resized = resize_wallet_artwork_2x(&input).unwrap();
        let decoded = image::load_from_memory(&resized).unwrap();
        use image::GenericImageView;
        assert_eq!(decoded.dimensions(), (1_024, 646));
    }

    #[test]
    fn cache_invalidation_uses_exactly_the_six_allowlisted_leaves() {
        let targets = cache_leaf_targets();
        assert_eq!(targets.len(), 6);
        assert_eq!(targets.iter().filter(|(suffix, _)| *suffix == ".cache").count(), 3);
        assert_eq!(targets.iter().filter(|(suffix, _)| *suffix == ".pkcache").count(), 3);
        assert!(targets.iter().all(|(_, leaf)| CACHE_FILES.contains(leaf)));
    }

    #[test]
    fn restoration_skips_only_exact_original_readback() {
        let expected = b"saved original";
        assert!(!original_needs_write(expected, Some(expected)));
        assert!(original_needs_write(expected, Some(b"different")));
        assert!(original_needs_write(expected, None));
    }
}
