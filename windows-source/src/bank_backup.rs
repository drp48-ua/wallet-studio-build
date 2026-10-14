//! Immutable PC-side baseline for bank-card artwork.
//!
//! Callers may save only after they have independently proved native manifest
//! capture/return and each local asset's metadata/return. This module verifies
//! and persists those bytes; it does not establish device-side proof itself.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::bank_artwork::{DeclaredArtwork, parse_manifest};
use crate::wallet_backup::CARD_ARTWORK_ASSETS;

const BUNDLE_VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_ARTWORK_BYTES: usize = crate::bank_artwork::MAX_ARTWORK_BYTES;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalAssetState {
    Absent,
    Present { bytes: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub udid: String,
    pub card_hash: String,
    pub manifest_bytes: Vec<u8>,
    pub artwork_name: String,
    pub remote_artwork_bytes: Vec<u8>,
    pub remote_size: usize,
    pub remote_sha1: String,
    pub local_assets: BTreeMap<String, LocalAssetState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DiskManifest {
    version: u32,
    udid: String,
    card_hash: String,
    manifest_sha256: String,
    artwork_name: String,
    artwork_sha256: String,
    artwork_size: usize,
    artwork_sha1: String,
    local_assets: BTreeMap<String, LocalAssetRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum LocalAssetRecord {
    Absent,
    Present { size: usize, sha256: String },
}

fn baseline_root() -> PathBuf {
    crate::storage::root().join("bank-backups")
}

pub(crate) fn bundle_identity(directory:&Path)->Result<(String,String)> {
    crate::storage::safe_chain(directory)?;
    crate::storage::safe_chain(&directory.join("baseline.json"))?;
    let metadata=fs::symlink_metadata(directory.join("baseline.json"))?;
    anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len()<=64*1024,"unsafe baseline metadata");
    let manifest:DiskManifest=serde_json::from_slice(&fs::read(directory.join("baseline.json"))?)?;
    let root=directory.parent().context("baseline parent missing")?;
    anyhow::ensure!(bundle_path(root,&manifest.udid,&manifest.card_hash)?==directory,"baseline identity directory mismatch");
    Ok((manifest.udid,manifest.card_hash))
}
pub(crate) fn validate_bundle_directory(directory:&Path)->Result<Baseline> {
    let (udid,hash)=bundle_identity(directory)?;
    load_baseline_at(directory.parent().context("baseline parent missing")?,&udid,&hash)?.context("baseline missing")
}

fn identity_key(udid: &str, card_hash: &str) -> Result<String> {
    anyhow::ensure!(!udid.is_empty() && udid.len() <= 256 && udid.trim() == udid,
        "Invalid device identity for bank-artwork baseline");
    anyhow::ensure!(crate::wallet_export::is_exact_card_hash(card_hash),
        "Invalid literal card hash for bank-artwork baseline");
    let mut identity = Vec::with_capacity(udid.len() + card_hash.len() + 1);
    identity.extend_from_slice(udid.as_bytes());
    identity.push(0);
    identity.extend_from_slice(card_hash.as_bytes());
    sha256(&identity)
}

fn bundle_path(root: &Path, udid: &str, card_hash: &str) -> Result<PathBuf> {
    Ok(root.join(identity_key(udid, card_hash)?))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)
        .with_context(|| format!("Refusing to overwrite baseline evidence at {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    anyhow::ensure!(fs::read(path)? == bytes, "Baseline file failed PC readback verification");
    Ok(())
}

fn validate_baseline(baseline: &Baseline) -> Result<(DeclaredArtwork, DiskManifest)> {
    anyhow::ensure!(identity_key(&baseline.udid, &baseline.card_hash).is_ok(),
        "Baseline device/card identity is invalid");
    anyhow::ensure!(!baseline.manifest_bytes.is_empty() && baseline.manifest_bytes.len() <= MAX_MANIFEST_BYTES,
        "Captured artwork manifest is empty or too large");
    anyhow::ensure!(baseline.remote_artwork_bytes.len() <= MAX_ARTWORK_BYTES,
        "Remote artwork exceeds the size limit");
    anyhow::ensure!(baseline.local_assets.len() == CARD_ARTWORK_ASSETS.len()
        && CARD_ARTWORK_ASSETS.iter().all(|leaf| baseline.local_assets.contains_key(*leaf)),
        "Baseline must record the exact three local artwork paths");

    let declarations = parse_manifest(&baseline.manifest_bytes)
        .context("Saved bank-artwork manifest is invalid")?;
    let declaration = declarations.iter().find(|entry| entry.name == baseline.artwork_name)
        .context("Saved manifest does not declare the selected remote artwork")?;
    anyhow::ensure!(declaration.size == baseline.remote_size
        && declaration.sha1.eq_ignore_ascii_case(&baseline.remote_sha1),
        "Remote artwork declaration does not match the saved manifest");
    anyhow::ensure!(baseline.remote_artwork_bytes.len() == declaration.size,
        "Remote artwork byte count does not match its manifest declaration");
    anyhow::ensure!(sha1(&baseline.remote_artwork_bytes)? == declaration.sha1.to_ascii_lowercase(),
        "Remote artwork SHA-1 does not match its manifest declaration");
    validate_remote_format(&baseline.artwork_name, &baseline.remote_artwork_bytes)?;

    let mut records = BTreeMap::new();
    for leaf in CARD_ARTWORK_ASSETS {
        match &baseline.local_assets[leaf] {
            LocalAssetState::Absent => { records.insert(leaf.to_string(), LocalAssetRecord::Absent); }
            LocalAssetState::Present { bytes } => {
                // Presence is a preservation fact, not a promise that the
                // bytes decode as artwork. Keep opaque/empty originals exact;
                // preview and apply code must validate formats separately.
                anyhow::ensure!(bytes.len() <= MAX_ARTWORK_BYTES,
                    "Captured local artwork exceeds the size limit: {leaf}");
                records.insert(leaf.to_string(), LocalAssetRecord::Present {
                    size: bytes.len(), sha256: sha256(bytes)?,
                });
            }
        }
    }
    let disk = DiskManifest {
        version: BUNDLE_VERSION,
        udid: baseline.udid.clone(),
        card_hash: baseline.card_hash.clone(),
        manifest_sha256: sha256(&baseline.manifest_bytes)?,
        artwork_name: baseline.artwork_name.clone(),
        artwork_sha256: sha256(&baseline.remote_artwork_bytes)?,
        artwork_size: baseline.remote_artwork_bytes.len(),
        artwork_sha1: declaration.sha1.to_ascii_lowercase(),
        local_assets: records,
    };
    Ok((declaration.clone(), disk))
}

fn validate_remote_format(name: &str, bytes: &[u8]) -> Result<()> {
    anyhow::ensure!(bytes.len() <= MAX_ARTWORK_BYTES, "Remote artwork exceeds the size limit");
    if name.ends_with(".pdf") {
        anyhow::ensure!(bytes.starts_with(b"%PDF-") && bytes.windows(5).any(|w| w == b"%%EOF"),
            "Remote PDF artwork is structurally invalid");
    } else {
        let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format().context("Remote artwork format is unknown")?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8_192);
        limits.max_image_height = Some(8_192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().context("Remote artwork image is invalid or unsupported")?;
        anyhow::ensure!(image.width() > 0 && image.height() > 0, "Remote artwork has empty dimensions");
    }
    Ok(())
}

fn save_baseline_at(root: &Path, baseline: &Baseline) -> Result<PathBuf> {
    let (declaration, disk) = validate_baseline(baseline)?;
    crate::storage::safe_chain(root)?;fs::create_dir_all(root)?;
    let root_meta = fs::symlink_metadata(root)?;
    anyhow::ensure!(root_meta.is_dir() && !root_meta.file_type().is_symlink(),
        "Bank-artwork backup root must be a real directory, not a symlink");
    let directory = bundle_path(root, &baseline.udid, &baseline.card_hash)?;
    fs::create_dir(&directory).context("A bank-artwork baseline already exists or has incomplete evidence")?;

    write_new(&directory.join("manifest.urls"), &baseline.manifest_bytes)?;
    write_new(&directory.join("remote-artwork.bin"), &baseline.remote_artwork_bytes)?;
    for leaf in CARD_ARTWORK_ASSETS {
        if let LocalAssetState::Present { bytes } = &baseline.local_assets[leaf] {
            write_new(&directory.join(leaf), bytes)?;
        }
    }
    // Commit record is written last. Any interruption before this leaves an
    // incomplete directory that fails closed and is never overwritten.
    anyhow::ensure!(declaration.size == disk.artwork_size, "Internal artwork size mismatch");
    write_new(&directory.join("baseline.json"), &serde_json::to_vec_pretty(&disk)?)?;
    Ok(directory)
}

fn load_baseline_at(root: &Path, udid: &str, card_hash: &str) -> Result<Option<Baseline>> {
    let directory = bundle_path(root, udid, card_hash)?;
    crate::storage::safe_chain(&directory)?;
    if !directory.exists() { return Ok(None); }
    anyhow::ensure!(!fs::symlink_metadata(&directory)?.file_type().is_symlink(),
        "Baseline directory must not be a symbolic link");
    let disk: DiskManifest = serde_json::from_slice(&read_regular(&directory.join("baseline.json"))?)
        .context("Bank-artwork baseline metadata is corrupt")?;
    anyhow::ensure!(disk.version == BUNDLE_VERSION && disk.udid == udid && disk.card_hash == card_hash,
        "Bank-artwork baseline belongs to a different device/card or version");
    anyhow::ensure!(disk.local_assets.len() == CARD_ARTWORK_ASSETS.len()
        && CARD_ARTWORK_ASSETS.iter().all(|leaf| disk.local_assets.contains_key(*leaf)),
        "Baseline local asset set is invalid");

    let manifest_bytes = read_regular(&directory.join("manifest.urls"))?;
    anyhow::ensure!(manifest_bytes.len() <= MAX_MANIFEST_BYTES,
        "Saved artwork manifest exceeds the 1 MiB limit");
    anyhow::ensure!(sha256(&manifest_bytes)? == disk.manifest_sha256, "Saved manifest failed SHA-256 verification");
    let remote_artwork_bytes = read_regular(&directory.join("remote-artwork.bin"))?;
    anyhow::ensure!(remote_artwork_bytes.len() == disk.artwork_size
        && sha256(&remote_artwork_bytes)? == disk.artwork_sha256,
        "Saved remote artwork failed size/SHA-256 verification");

    let declarations = parse_manifest(&manifest_bytes).context("Saved manifest is invalid")?;
    let declaration = declarations.iter().find(|entry| entry.name == disk.artwork_name)
        .context("Saved manifest does not declare the saved artwork")?;
    anyhow::ensure!(declaration.size == disk.artwork_size
        && declaration.sha1.eq_ignore_ascii_case(&disk.artwork_sha1)
        && sha1(&remote_artwork_bytes)? == disk.artwork_sha1,
        "Saved artwork does not match the manifest size/SHA-1 declaration");
    validate_remote_format(&disk.artwork_name, &remote_artwork_bytes)?;

    let mut local_assets = BTreeMap::new();
    for leaf in CARD_ARTWORK_ASSETS {
        let state = match disk.local_assets.get(leaf).context("Baseline asset record missing")? {
            LocalAssetRecord::Absent => {
                anyhow::ensure!(!directory.join(leaf).exists(), "Unexpected bytes exist for an absent local asset");
                LocalAssetState::Absent
            }
            LocalAssetRecord::Present { size, sha256: expected } => {
                let bytes = read_regular(&directory.join(leaf))?;
                anyhow::ensure!(bytes.len() == *size && sha256(&bytes)? == *expected,
                    "Saved local asset failed size/SHA-256 verification: {leaf}");
                LocalAssetState::Present { bytes }
            }
        };
        local_assets.insert(leaf.to_string(), state);
    }
    Ok(Some(Baseline {
        udid: disk.udid, card_hash: disk.card_hash, manifest_bytes,
        artwork_name: disk.artwork_name, remote_artwork_bytes,
        remote_size: disk.artwork_size, remote_sha1: disk.artwork_sha1,
        local_assets,
    }))
}

fn read_regular(path: &Path) -> Result<Vec<u8>> {
    crate::storage::safe_chain(path)?;
    let metadata = fs::symlink_metadata(path).with_context(|| format!("Missing baseline file {}", path.display()))?;
    anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink(),
        "Baseline artifact is not a regular file: {}", path.display());
    anyhow::ensure!(metadata.len() <= MAX_ARTWORK_BYTES as u64,
        "Baseline artifact exceeds the size limit: {}", path.display());
    anyhow::ensure!(metadata.len() <= usize::MAX as u64,
        "Baseline artifact is too large for this system: {}", path.display());
    Ok(fs::read(path)?)
}

/// Save an immutable, verified baseline. A second save for the same identity
/// fails instead of replacing the first evidence set.
pub fn save_baseline(baseline: &Baseline) -> Result<PathBuf> {
    let _lease=crate::storage::operation_lease()?;
    save_baseline_at(&baseline_root(), baseline)
}

#[cfg(test)] pub(crate) fn save_fixture_at(root:&Path,baseline:&Baseline)->Result<PathBuf> { save_baseline_at(root,baseline) }

/// Load and fully verify a baseline, returning None only when no bundle exists.
pub fn load_baseline(udid: &str, card_hash: &str) -> Result<Option<Baseline>> {
    load_baseline_at(&baseline_root(), udid, card_hash)
}

pub fn baseline_exists(udid: &str, card_hash: &str) -> bool {
    load_baseline(udid, card_hash).is_ok_and(|baseline| baseline.is_some())
}

/// Find a previously verified local baseline for an offline card preview.
/// Device writes must still use the identity of the currently connected phone.
pub fn verified_baseline_device_for_card(card_hash: &str) -> Option<String> {
    verified_baseline_device_for_card_at(&baseline_root(), card_hash)
}

fn verified_baseline_device_for_card_at(root: &Path, card_hash: &str) -> Option<String> {
    if !crate::wallet_export::is_exact_card_hash(card_hash) {
        return None;
    }
    let root_meta = fs::symlink_metadata(root).ok()?;
    if !root_meta.is_dir() || root_meta.file_type().is_symlink() {
        return None;
    }
    for entry in fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else { continue };
        if !metadata.is_dir() || metadata.file_type().is_symlink() { continue; }
        let manifest_path = path.join("baseline.json");
        let Ok(manifest_meta) = fs::symlink_metadata(&manifest_path) else { continue };
        if !manifest_meta.is_file() || manifest_meta.file_type().is_symlink()
            || manifest_meta.len() > 64 * 1024 { continue; }
        let Ok(manifest_bytes) = fs::read(&manifest_path) else { continue };
        let Ok(disk) = serde_json::from_slice::<DiskManifest>(&manifest_bytes) else { continue };
        if disk.card_hash != card_hash { continue; }
        if load_baseline_at(root, &disk.udid, card_hash).is_ok_and(|baseline| baseline.is_some()) {
            return Some(disk.udid);
        }
    }
    None
}

pub fn baseline_has_three_local_originals(udid: &str, card_hash: &str) -> bool {
    load_baseline(udid, card_hash).ok().flatten().is_some_and(|baseline|
        baseline.local_assets.len() == CARD_ARTWORK_ASSETS.len()
            && CARD_ARTWORK_ASSETS.iter().all(|leaf| baseline.local_assets.contains_key(*leaf)))
}

/// Path to the verified remote artwork bytes for preview; never a device proof.
pub fn preview_path(udid: &str, card_hash: &str) -> Result<Option<PathBuf>> {
    if load_baseline(udid, card_hash)?.is_none() { return Ok(None); }
    Ok(Some(bundle_path(&baseline_root(), udid, card_hash)?.join("remote-artwork.bin")))
}

pub fn baseline_path(udid: &str, card_hash: &str) -> Result<Option<PathBuf>> {
    if load_baseline(udid, card_hash)?.is_none() { return Ok(None); }
    Ok(Some(bundle_path(&baseline_root(), udid, card_hash)?))
}

#[link(name = "bcrypt")]
unsafe extern "system" {
    fn BCryptOpenAlgorithmProvider(handle: *mut *mut std::ffi::c_void, name: *const u16,
        implementation: *const u16, flags: u32) -> i32;
    fn BCryptHash(handle: *mut std::ffi::c_void, secret: *const u8, secret_len: u32,
        input: *const u8, input_len: u32, output: *mut u8, output_len: u32) -> i32;
    fn BCryptCloseAlgorithmProvider(handle: *mut std::ffi::c_void, flags: u32) -> i32;
}

fn digest(bytes: &[u8], algorithm_name: &str, output: &mut [u8]) -> Result<()> {
    let name: Vec<u16> = format!("{algorithm_name}\0").encode_utf16().collect();
    let mut algorithm = std::ptr::null_mut();
    unsafe {
        anyhow::ensure!(BCryptOpenAlgorithmProvider(&mut algorithm, name.as_ptr(), std::ptr::null(), 0) == 0,
            "Could not initialize {algorithm_name} verification");
        let status = BCryptHash(algorithm, std::ptr::null(), 0, bytes.as_ptr(),
            u32::try_from(bytes.len())?, output.as_mut_ptr(), output.len() as u32);
        BCryptCloseAlgorithmProvider(algorithm, 0);
        anyhow::ensure!(status == 0, "Could not compute {algorithm_name} digest");
    }
    Ok(())
}

pub(crate) fn sha256(bytes: &[u8]) -> Result<String> {
    let mut output = [0u8; 32];
    digest(bytes, "SHA256", &mut output)?;
    Ok(output.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub(crate) fn sha1(bytes: &[u8]) -> Result<String> {
    let mut output = [0u8; 20];
    digest(bytes, "SHA1", &mut output)?;
    Ok(output.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(3, 3).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    fn baseline() -> Baseline {
        let remote = png();
        let remote_sha1 = sha1(&remote).unwrap();
        let manifest_bytes = serde_json::to_vec(&serde_json::json!({
            "cardBackgroundCombined@2x.png": {"url":"https://a.apple.com/art", "size":remote.len(), "sha1":remote_sha1}
        })).unwrap();
        let local_assets = CARD_ARTWORK_ASSETS.iter().map(|leaf| (leaf.to_string(),
            if *leaf == "cardBackgroundCombined@3x.png" {
                LocalAssetState::Present { bytes: png() }
            } else { LocalAssetState::Absent }
        )).collect();
        Baseline { udid: "device-A".into(), card_hash: "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=".into(),
            manifest_bytes, artwork_name: "cardBackgroundCombined@2x.png".into(),
            remote_artwork_bytes: remote.clone(), remote_size: remote.len(), remote_sha1, local_assets }
    }

    #[test]
    fn immutable_baseline_round_trip_and_second_save_refuses_overwrite() {
        let root = std::env::temp_dir().join(format!("bank-baseline-test-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let input = baseline();
        let path = save_baseline_at(&root, &input).unwrap();
        let loaded = load_baseline_at(&root, &input.udid, &input.card_hash).unwrap().unwrap();
        assert_eq!(loaded, input);
        assert!(save_baseline_at(&root, &input).is_err());
        assert!(!path.file_name().unwrap().to_string_lossy().contains(&input.udid));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn offline_lookup_only_returns_an_intact_baseline_for_the_card() {
        let root = std::env::temp_dir().join(format!("bank-baseline-offline-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let input = baseline();
        let path = save_baseline_at(&root, &input).unwrap();
        assert_eq!(verified_baseline_device_for_card_at(&root, &input.card_hash), Some(input.udid.clone()));
        assert_eq!(verified_baseline_device_for_card_at(&root, "invalid"), None);
        fs::write(path.join("remote-artwork.bin"), b"tampered").unwrap();
        assert_eq!(verified_baseline_device_for_card_at(&root, &input.card_hash), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tampering_or_cross_device_identity_fails_closed() {
        let root = std::env::temp_dir().join(format!("bank-baseline-tamper-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let input = baseline();
        let path = save_baseline_at(&root, &input).unwrap();
        assert!(load_baseline_at(&root, "device-B", &input.card_hash).unwrap().is_none());
        fs::write(path.join("remote-artwork.bin"), b"tampered").unwrap();
        assert!(load_baseline_at(&root, &input.udid, &input.card_hash).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn baseline_rejects_local_state_gaps_and_wrong_remote_manifest_hash() {
        let root = std::env::temp_dir().join(format!("bank-baseline-invalid-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let mut input = baseline();
        input.local_assets.remove("cardBackgroundCombined.pdf");
        assert!(save_baseline_at(&root, &input).is_err());
        input = baseline();
        input.remote_sha1 = "0000000000000000000000000000000000000000".into();
        assert!(save_baseline_at(&root, &input).is_err());
        assert!(!root.exists());
    }

    #[test]
    fn baseline_preserves_empty_and_opaque_local_originals_exactly() {
        let root = std::env::temp_dir().join(format!("bank-baseline-opaque-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let mut input = baseline();
        input.local_assets.insert("cardBackgroundCombined@3x.png".into(),
            LocalAssetState::Present { bytes: Vec::new() });
        input.local_assets.insert("cardBackgroundCombined@2x.png".into(),
            LocalAssetState::Present { bytes: b"not-an-image-but-original-bytes".to_vec() });
        let _ = save_baseline_at(&root, &input).unwrap();
        assert_eq!(load_baseline_at(&root, &input.udid, &input.card_hash).unwrap(), Some(input));
        fs::remove_dir_all(root).unwrap();
    }
}
