use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::device::ConnectionMode;

pub const CARD_ARTWORK_ASSETS: [&str; 3] = [
    "cardBackgroundCombined@3x.png",
    "cardBackgroundCombined@2x.png",
    "cardBackgroundCombined.pdf",
];

fn backup_root() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| r"C:\Users\Default\AppData\Local".to_string());
    PathBuf::from(local_app_data)
        .join("AirCard")
        .join("wallet-backups")
}

fn backup_dir(udid: &str, card_hash: &str) -> PathBuf {
    backup_root().join(format!(
        "{}-{}",
        safe_component(udid),
        stable_hash(card_hash)
    ))
}

fn safe_component(value: &str) -> String {
    let component: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if component.is_empty() {
        "unknown".to_string()
    } else {
        component
    }
}

fn stable_hash(value: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub fn backup_exists(udid: &str, card_hash: &str) -> bool {
    let dir = backup_dir(udid, card_hash);
    complete_backup_at(&dir)
}

/// Existing or uncertain evidence must not be replaced by an automatic capture.
/// This does not certify the contents; use backup_exists for that separately.
pub fn backup_evidence_exists(udid: &str, card_hash: &str) -> bool {
    backup_evidence_at(&backup_dir(udid, card_hash))
}

fn backup_evidence_at(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        _ => true,
    }
}

/// A preview is a local original, never a claim about the current phone image.
pub fn preview_original_path(udid: &str, card_hash: &str) -> Option<PathBuf> {
    let dir = backup_dir(udid, card_hash);
    for asset in &CARD_ARTWORK_ASSETS[..2] {
        let path = dir.join(asset);
        if fs::read(&path).ok().and_then(|bytes| image::load_from_memory(&bytes).ok()).is_some() {
            return Some(path);
        }
    }
    None
}

pub(crate) fn valid_asset(asset: &str, bytes: &[u8]) -> bool {
    if bytes.is_empty() || bytes.len() > 32 * 1024 * 1024 { return false; }
    if asset.ends_with(".png") {
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        reader.decode().is_ok_and(|image| image.width() > 0 && image.height() > 0)
    } else {
        bytes.starts_with(b"%PDF-") && bytes.windows(5).any(|part| part == b"%%EOF")
    }
}

fn complete_backup_at(dir: &Path) -> bool {
    let Ok(manifest_bytes) = fs::read(dir.join("verified-original.json")) else { return false };
    let Ok(manifest) = serde_json::from_slice::<BackupManifest>(&manifest_bytes) else { return false };
    manifest.version == 1 && manifest.assets.len() == CARD_ARTWORK_ASSETS.len()
        && CARD_ARTWORK_ASSETS.iter().all(|asset| fs::read(dir.join(asset)).is_ok_and(|bytes|
            valid_asset(asset, &bytes) && manifest.assets.get(*asset)
                .is_some_and(|hash| sha256(&bytes).is_ok_and(|actual| actual == *hash))))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BackupManifest {
    version: u32,
    assets: std::collections::BTreeMap<String, String>,
}

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptOpenAlgorithmProvider(handle: *mut *mut std::ffi::c_void, name: *const u16,
        implementation: *const u16, flags: u32) -> i32;
    fn BCryptHash(handle: *mut std::ffi::c_void, secret: *const u8, secret_len: u32,
        input: *const u8, input_len: u32, output: *mut u8, output_len: u32) -> i32;
    fn BCryptCloseAlgorithmProvider(handle: *mut std::ffi::c_void, flags: u32) -> i32;
}

fn sha256(bytes: &[u8]) -> Result<String> {
    let input_len = u32::try_from(bytes.len())?;
    let name: Vec<u16> = "SHA256\0".encode_utf16().collect();
    let mut algorithm = std::ptr::null_mut();
    let mut digest = [0u8; 32];
    unsafe {
        anyhow::ensure!(BCryptOpenAlgorithmProvider(&mut algorithm, name.as_ptr(), std::ptr::null(), 0) == 0,
            "No se pudo iniciar la verificación SHA-256");
        let status = BCryptHash(algorithm, std::ptr::null(), 0, bytes.as_ptr(), input_len,
            digest.as_mut_ptr(), digest.len() as u32);
        BCryptCloseAlgorithmProvider(algorithm, 0);
        anyhow::ensure!(status == 0, "No se pudo verificar la integridad del original");
    }
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn commit_verified_manifest(dir: &Path) -> Result<()> {
    let mut assets = std::collections::BTreeMap::new();
    for asset in CARD_ARTWORK_ASSETS {
        let bytes = fs::read(dir.join(asset))?;
        anyhow::ensure!(valid_asset(asset, &bytes), "Original incompleto: {}", asset);
        assets.insert(asset.to_string(), sha256(&bytes)?);
    }
    write_backup_file(&dir.join("verified-original.json"),
        &serde_json::to_vec_pretty(&BackupManifest { version: 1, assets })?)
}

pub fn capture_original_card<L>(
    udid: &str,
    connection_mode: ConnectionMode,
    card_hash: &str,
    mut log: L,
) -> Result<Option<String>>
where
    L: FnMut(&str),
{
    anyhow::ensure!(crate::scanner::is_valid_card_hash(card_hash),
        "El identificador de la tarjeta no es válido");
    let dir = backup_dir(udid, card_hash);
    fs::create_dir_all(&dir)
        .with_context(|| format!("Could not create backup directory {}", dir.display()))?;
    if complete_backup_at(&dir) {
        log("Copia original completa: integridad SHA-256 verificada.");
        return Ok(Some(card_hash.to_string()));
    }

    log("Exporting the original Wallet artwork through Airlift with a durable PC recovery copy...");
    for asset in CARD_ARTWORK_ASSETS {
        let backup_path = dir.join(asset);
        // Old loose files remain previews only. Every current source asset is
        // exported and preserved before the backup manifest can be certified.
        let data = crate::wallet_export::capture_verified_original_asset(
            udid, connection_mode, card_hash, asset, &mut log)
            .with_context(|| format!("Could not safely export original card asset {}", asset))?;
        anyhow::ensure!(valid_asset(asset, &data), "El original {} no es una imagen válida", asset);
        write_backup_file(&backup_path, &data)
            .with_context(|| format!("Failed to save original card asset {}", asset))?;
        log(&format!("Backed up original card asset: {}", asset));
    }

    commit_verified_manifest(&dir)?;
    Ok(Some(card_hash.to_string()))
}

pub fn load_original_assets(
    udid: &str,
    card_hash: &str,
) -> Result<Vec<(String, Vec<u8>)>> {
    anyhow::ensure!(crate::wallet_export::is_exact_card_hash(card_hash),
        "El identificador de la tarjeta no es valido");
    let dir = backup_dir(udid, card_hash);
    anyhow::ensure!(complete_backup_at(&dir),
        "Falta una copia original completa con integridad verificada. No se modificará la tarjeta.");
    let mut assets = Vec::new();

    for asset in CARD_ARTWORK_ASSETS {
        let path = dir.join(asset);
        if !path.is_file() {
            continue;
        }
        let data = fs::read(&path)
            .with_context(|| format!("Could not read backup file {}", path.display()))?;
        assets.push((asset.to_string(), data));
    }

    if assets.is_empty() {
        bail!("Original card face backup not found for card hash {}", card_hash);
    }

    anyhow::ensure!(assets.len() == CARD_ARTWORK_ASSETS.len()
        && assets.iter().all(|(name, bytes)| valid_asset(name, bytes)),
        "Falta una copia original completa y válida. No se modificará la tarjeta.");

    Ok(assets)
}

fn write_backup_file(path: &Path, data: &[u8]) -> Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, data)?;
    anyhow::ensure!(fs::read(&temporary)? == data, "La copia local no coincide con los bytes originales");
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_original_evidence_blocks_automatic_recapture() {
        let dir = std::env::temp_dir().join(format!("wallet-studio-evidence-test-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        assert!(!backup_evidence_at(&dir));
        fs::create_dir(&dir).unwrap();
        assert!(backup_evidence_at(&dir));
        assert!(!complete_backup_at(&dir));
        fs::write(dir.join("verified-original.json"), b"incomplete").unwrap();
        assert!(backup_evidence_at(&dir));
        assert!(!complete_backup_at(&dir));
        fs::remove_file(dir.join("verified-original.json")).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn corrupt_or_partial_assets_do_not_enable_restore() {
        assert!(!valid_asset("cardBackgroundCombined@3x.png", b"not a PNG"));
        assert!(!valid_asset("cardBackgroundCombined.pdf", b"%PDF-1.4 truncated"));
        assert!(!complete_backup_at(&std::env::temp_dir().join("wallet-studio-nonexistent-test-backup")));
    }
    #[test]
    fn oversized_png_dimensions_are_rejected_before_allocation() {
        let image = image::DynamicImage::new_rgb8(8193, 1);
        let mut bytes = Vec::new();
        image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        assert!(!valid_asset("cardBackgroundCombined@3x.png", &bytes));
        assert!(!valid_asset("cardBackgroundCombined.pdf", &vec![0; 32 * 1024 * 1024 + 1]));
    }
    #[test]
    fn checksum_matches_standard_sha256_vector() {
        assert_eq!(sha256(b"abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
    #[test]
    fn modified_backup_cannot_pass_original_integrity_check() {
        let dir = std::env::temp_dir().join(format!("wallet-studio-backup-test-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        fs::create_dir(&dir).unwrap();
        let mut png = Vec::new();
        image::DynamicImage::new_rgb8(2, 2).write_to(&mut std::io::Cursor::new(&mut png),
            image::ImageFormat::Png).unwrap();
        let pdf = crate::image_skin::png_to_pdf(&png).unwrap();
        for asset in CARD_ARTWORK_ASSETS {
            fs::write(dir.join(asset), if asset.ends_with(".png") { &png } else { &pdf }).unwrap();
        }
        // Old loose files cannot grant write permission.
        assert!(!complete_backup_at(&dir));
        commit_verified_manifest(&dir).unwrap();
        assert!(complete_backup_at(&dir));
        fs::write(dir.join(CARD_ARTWORK_ASSETS[0]), b"changed after capture").unwrap();
        assert!(!complete_backup_at(&dir));
        for asset in CARD_ARTWORK_ASSETS { fs::remove_file(dir.join(asset)).unwrap(); }
        fs::remove_file(dir.join("verified-original.json")).unwrap();
        fs::remove_dir(dir).unwrap();
    }
}
