//! Recoverable export of protected Wallet artwork through the Books/AirTraffic
//! move. Every operation gets a private Media namespace and a PC journal.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::afc::AfcClient;
use crate::airlift::{LINK_PREFIX, RECOVERED_PREFIX, SOURCE_PREFIX, build_books_plist,
    build_streaming_zip_archive, restore_books, snapshot_books, stage_streaming_zip};
use crate::airtraffic::sync_assets_via_airtraffic;
use crate::device::{ActiveDeviceSession, ConnectionMode};

const MAX_ARTWORK_BYTES: usize = 32 * 1024 * 1024;
pub const BANK_MANIFEST: &str = "cardBackgroundCombined.png.urls";
const CACHE_LEAVES: [&str; 3] = ["FrontFace", "PlaceHolder", "Preview"];

fn default_bundle_suffix() -> String { ".pkpass".into() }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MetadataState { Absent, Present { size: usize }, Unreadable }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemovalPostcondition { ConfirmedAbsent, PreserveEvidence }

fn classify_removal_postcondition(state: &MetadataState) -> RemovalPostcondition {
    if matches!(state, MetadataState::Absent) {
        RemovalPostcondition::ConfirmedAbsent
    } else {
        RemovalPostcondition::PreserveEvidence
    }
}

fn valid_resource(leaf: &str, bytes: &[u8]) -> bool {
    if leaf != BANK_MANIFEST { return crate::wallet_backup::valid_asset(leaf, bytes); }
    if bytes.is_empty() || bytes.len() > 1024 * 1024 { return false; }
    serde_json::from_slice::<serde_json::Value>(bytes).is_ok_and(|value| value.is_object())
        || plist::Value::from_reader(std::io::Cursor::new(bytes))
            .is_ok_and(|value| value.as_dictionary().is_some())
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Journal {
    version: u32,
    revision: u32,
    token: String,
    udid: String,
    card_hash: String,
    #[serde(default = "default_bundle_suffix")]
    bundle_suffix: String,
    leaf: String,
    phase: String,
    original_sha256: Option<String>,
    books: BTreeMap<String, Option<Vec<u8>>>,
    device_tokens: Vec<String>,
}

fn journal_root() -> PathBuf {
    crate::storage::root().join("wallet-export-recovery")
}

fn create_operation_dir(root: &Path, token: &str) -> Result<PathBuf> {
    anyhow::ensure!(valid_token(token), "Invalid operation token");
    fs::create_dir_all(root)?;
    let operation_dir = root.join(token);
    fs::create_dir(&operation_dir)?;
    Ok(operation_dir)
}

fn durable_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Recovery path has no parent")?;
    fs::create_dir_all(parent)?;
    let temp = path.with_extension("pending");
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&temp)?;
    use std::io::Write;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    anyhow::ensure!(fs::read(&temp)? == bytes, "PC recovery copy failed readback verification");
    anyhow::ensure!(!path.exists(), "Refusing to replace existing recovery evidence");
    fs::rename(temp, path)?;
    Ok(())
}

fn save_journal(path: &Path, journal: &mut Journal) -> Result<()> {
    journal.revision = journal.revision.checked_add(1).context("Export journal revision overflow")?;
    let state_path = path.join(format!("state-{:04}-{}.json", journal.revision, journal.phase));
    durable_write(&state_path, &serde_json::to_vec_pretty(journal)?)
}

fn airlock_relpath(absolute: &str) -> String {
    let trimmed = absolute.trim_start_matches('/');
    let tail = trimmed.strip_prefix("var/mobile/").unwrap_or(trimmed);
    format!("../../../{tail}")
}

fn ensure_readback_matches(original: &[u8], readback: &[u8]) -> Result<()> {
    anyhow::ensure!(original == readback && sha256(original)? == sha256(readback)?,
        "Fresh protected readback differs from the PC original");
    Ok(())
}

fn preserve_exported_copy(operation_dir: &Path, journal: &mut Journal, leaf: &str,
    round: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = operation_dir.join(format!("{leaf}-{round}.original"));
    durable_write(&path, bytes).context("Could not durably preserve exported original on this PC")?;
    let readback = fs::read(&path)?;
    anyhow::ensure!(readback == bytes, "PC original copy failed readback verification");
    if journal.original_sha256.is_none() {
        journal.original_sha256 = Some(sha256(bytes)?);
    }
    journal.phase = format!("{round}-pc-copy-verified");
    save_journal(operation_dir, journal)?;
    Ok(path)
}

#[cfg(test)]
fn preserve_uncertain_export(operation_dir: &Path, journal: &mut Journal, leaf: &str,
    round: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = preserve_exported_copy(operation_dir, journal, leaf, round, bytes)?;
    journal.phase = format!("{round}-uncertain-pc-copy-preserved");
    save_journal(operation_dir, journal)?;
    Ok(path)
}

pub fn valid_leaf(leaf: &str) -> bool {
    crate::wallet_backup::CARD_ARTWORK_ASSETS.contains(&leaf) || leaf == BANK_MANIFEST
}

fn valid_suffix_leaf(suffix: &str, leaf: &str) -> bool {
    match suffix {
        ".pkpass" => valid_leaf(leaf),
        ".cache" | ".pkcache" => CACHE_LEAVES.contains(&leaf),
        _ => false,
    }
}

fn resource_limit(leaf: &str) -> usize {
    if leaf == BANK_MANIFEST { 1024 * 1024 } else { MAX_ARTWORK_BYTES }
}

fn bundle_target(card_hash: &str, suffix: &str) -> String {
    format!("/var/mobile/Library/Passes/Cards/{card_hash}{suffix}")
}

/// Card identifiers are used as a literal path component. Reject scanner inputs
/// that are only valid after trimming quotes or punctuation.
pub fn is_exact_card_hash(hash: &str) -> bool {
    !hash.is_empty()
        && hash == hash.trim()
        && hash.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'_' | b'='))
        && crate::scanner::is_valid_card_hash(hash)
}

fn sha256(bytes: &[u8]) -> Result<String> {
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptOpenAlgorithmProvider(handle: *mut *mut std::ffi::c_void, name: *const u16,
            implementation: *const u16, flags: u32) -> i32;
        fn BCryptHash(handle: *mut std::ffi::c_void, secret: *const u8, secret_len: u32,
            input: *const u8, input_len: u32, output: *mut u8, output_len: u32) -> i32;
        fn BCryptCloseAlgorithmProvider(handle: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    let name: Vec<u16> = "SHA256\0".encode_utf16().collect();
    let mut algorithm = std::ptr::null_mut();
    let mut digest = [0u8; 32];
    unsafe {
        anyhow::ensure!(BCryptOpenAlgorithmProvider(&mut algorithm, name.as_ptr(), std::ptr::null(), 0) == 0,
            "Could not start SHA-256 verification");
        let status = BCryptHash(algorithm, std::ptr::null(), 0, bytes.as_ptr(),
            u32::try_from(bytes.len())?, digest.as_mut_ptr(), digest.len() as u32);
        BCryptCloseAlgorithmProvider(algorithm, 0);
        anyhow::ensure!(status == 0, "Could not verify recovery copy");
    }
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// A pending export blocks new captures until its PC original and device state
/// have been reviewed. Paths are scoped to the operation directory.
#[derive(Debug, Clone)]
pub struct PendingRecovery {
    pub operation_dir: PathBuf,
    pub phase: String,
    pub udid: String,
    pub card_hash: String,
    pub leaf: String,
    pub original_path: Option<PathBuf>,
    pub baseline_recovery_available: bool,
}

/// Valid journals are scoped to their phone because Books state is per-device.
/// An unreadable ledger remains globally blocking; no selected phone is also
/// conservative. This never changes or completes any recovery evidence.
pub fn pending_recovery_blocks_device(pending: std::result::Result<&[PendingRecovery], ()>, selected: Option<&str>) -> bool {
    pending_recovery_blocks_device_except(pending, selected, None)
}

#[cfg(test)]
mod pending_device_tests {
    use super::*;
    fn pending(udid: &str, card: &str, directory: &str) -> PendingRecovery {
        PendingRecovery { operation_dir: PathBuf::from(directory), phase: "export-uncertain".into(),
            udid: udid.into(), card_hash: card.into(), leaf: "cardBackgroundCombined@2x.png".into(), original_path: None, baseline_recovery_available: false }
    }
    #[test]
    fn valid_foreign_pending_does_not_block_current_phone_or_require_an_original_file() {
        let ledger = vec![pending("phone-A", "card-A", "pending-A")];
        assert!(!pending_recovery_blocks_device(Ok(&ledger),Some("phone-B")));
        assert!(pending_recovery_blocks_device(Ok(&ledger),Some("PHONE-a")));
        assert!(pending_recovery_blocks_device(Ok(&ledger),None));
        assert!(ledger[0].original_path.is_none());
    }
    #[test]
    fn all_cards_on_current_phone_share_the_books_recovery_block_and_mixed_ledgers_block() {
        let same_phone_other_card = vec![pending("phone-B", "another-card", "pending-B")];
        assert!(pending_recovery_blocks_device(Ok(&same_phone_other_card),Some("phone-B")));
        let mixed = vec![pending("phone-A", "card-A", "pending-A"), pending("phone-B", "another-card", "pending-B")];
        assert!(pending_recovery_blocks_device(Ok(&mixed),Some("phone-B")));
        assert!(pending_recovery_blocks_device(Ok(&mixed),None));
        assert!(!pending_recovery_blocks_device(Ok(&mixed),Some("phone-C")));
    }
    #[test]
    fn unknown_or_corrupt_ledger_is_global_fail_closed_even_without_pending_rows() {
        for selected in [None,Some("phone-A"),Some("phone-B")] {
            assert!(pending_recovery_blocks_device(Err(()),selected));
        }
        assert!(!pending_recovery_blocks_device(Ok(&[]),None));
    }
    #[test]
    fn allowed_recovery_is_only_an_exact_operation_on_the_same_phone() {
        let current = pending("phone-B", "card-B", "pending-B");
        let foreign = pending("phone-A", "card-A", "pending-A");
        let ledger = vec![foreign,current];
        assert!(!pending_recovery_blocks_device_except(Ok(&ledger),Some("phone-B"),Some(Path::new("pending-B"))));
        assert!(pending_recovery_blocks_device_except(Ok(&ledger),Some("phone-B"),Some(Path::new("pending-A"))));
        assert!(pending_recovery_blocks_device_except(Ok(&ledger),Some("phone-B"),Some(Path::new("pending-B-different"))));
        assert!(pending_recovery_blocks_device_except(Err(()),Some("phone-B"),Some(Path::new("pending-B"))));
    }
}

fn pending_recovery_blocks_device_except(pending: std::result::Result<&[PendingRecovery], ()>,
    selected: Option<&str>, allowed_operation: Option<&Path>) -> bool {
    match pending {
        Err(()) => true,
        Ok(pending) => pending.iter().any(|operation|
            selected.is_none_or(|udid| operation.udid.eq_ignore_ascii_case(udid))
                && allowed_operation != Some(operation.operation_dir.as_path())),
    }
}

fn latest_journal(operation_dir: &Path) -> Result<Journal> {
    let mut states = fs::read_dir(operation_dir)?.filter_map(|item| item.ok())
        .map(|item| item.path())
        .filter(|path| path.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.starts_with("state-") && name.ends_with(".json")))
        .collect::<Vec<_>>();
    states.sort();
    let path = states.pop().context("No durable export state found")?;
    let journal: Journal = serde_json::from_slice(&fs::read(path)?).context("Export recovery state is invalid")?;
    validate_journal(&journal)?;
    Ok(journal)
}

fn valid_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_journal(journal: &Journal) -> Result<()> {
    anyhow::ensure!(journal.version == 1 && valid_token(&journal.token), "Unsupported or invalid recovery journal identity");
    anyhow::ensure!(is_exact_card_hash(&journal.card_hash), "Invalid card hash in recovery journal");
    anyhow::ensure!(valid_suffix_leaf(&journal.bundle_suffix, &journal.leaf), "Invalid resource target in recovery journal");
    anyhow::ensure!(!journal.udid.is_empty() && journal.udid.len() <= 256, "Invalid device identity in recovery journal");
    anyhow::ensure!(journal.device_tokens.iter().all(|token| valid_token(token)), "Invalid Airlift token in recovery journal");
    anyhow::ensure!(journal.books.len() == crate::airlift::TRACKED_BOOKS_FILES.len()
        && crate::airlift::TRACKED_BOOKS_FILES.iter().all(|path| journal.books.contains_key(*path)),
        "Invalid Books preimage paths in recovery journal");
    if let Some(hash) = journal.original_sha256.as_deref() {
        anyhow::ensure!(hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()), "Invalid original hash in recovery journal");
    }
    Ok(())
}

fn validate_operation_dir(operation_dir: &Path, journal: &Journal) -> Result<()> {
    validate_operation_dir_at(operation_dir,journal,&journal_root())
}
fn validate_operation_dir_at(operation_dir:&Path,journal:&Journal,root:&Path)->Result<()> {
    crate::storage::safe_chain(operation_dir)?;
    anyhow::ensure!(operation_dir.parent() == Some(root)
        && operation_dir.file_name().and_then(|name| name.to_str()) == Some(journal.token.as_str()),
        "Recovery directory does not match the journal token");
    anyhow::ensure!(!fs::symlink_metadata(operation_dir)?.file_type().is_symlink(),
        "Recovery directory cannot be a symbolic link");
    Ok(())
}

pub fn pending_recovery_operations() -> Result<Vec<PendingRecovery>> {
    pending_recovery_operations_at(&journal_root())
}
pub(crate) fn pending_recovery_operations_at(root:&Path)->Result<Vec<PendingRecovery>> {
    crate::storage::safe_chain(root)?;
    if !root.exists() { return Ok(Vec::new()); }
    let mut pending = Vec::new();
    for item in fs::read_dir(root)? {
        let path = item?.path();
        if !path.is_dir() { continue; }
        // Empty directories may remain if the process stopped between mkdir and
        // writing its first journal. They contain no device token or evidence.
        let has_state = fs::read_dir(&path)?.filter_map(|entry| entry.ok()).any(|entry|
            entry.file_name().to_string_lossy().starts_with("state-")
                || entry.file_name().to_string_lossy().ends_with(".original"));
        if !has_state { continue; }
        let journal = latest_journal(&path)?;
        validate_operation_dir_at(&path, &journal,root)?;
        if journal.phase == "complete" { continue; }
        let mut original_path = None;
        if let Some(expected) = journal.original_sha256.as_deref() {
            for candidate in original_candidates(&path, &journal.leaf)? {
        if fs::symlink_metadata(&candidate).is_ok_and(|meta| !meta.file_type().is_symlink()) {
            if let Ok(bytes) = fs::read(&candidate) {
                    if sha256(&bytes).is_ok_and(|actual| actual == expected) { original_path = Some(candidate); }
            }
                }
            }
        }
        let baseline_recovery_available = recovery_baseline(&journal).is_ok();
        pending.push(PendingRecovery { operation_dir: path, phase: journal.phase,
            udid: journal.udid, card_hash: journal.card_hash, leaf: journal.leaf, original_path, baseline_recovery_available });
    }
    Ok(pending)
}

fn original_candidates(operation_dir: &Path, leaf: &str) -> Result<Vec<PathBuf>> {
    let prefix = format!("{leaf}-");
    Ok(fs::read_dir(operation_dir)?.filter_map(|item| item.ok().map(|item| item.path()))
        .filter(|path| path.file_name().and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".original")))
        .collect())
}

/// Load a PC-preserved original from a pending operation after validating its
/// immutable journal hash. This never contacts or changes a device.
pub fn load_preserved_original(operation_dir: &Path) -> Result<Vec<u8>> {
    let journal = latest_journal(operation_dir)?;
    validate_operation_dir(operation_dir, &journal)?;
    verified_original_in(operation_dir, &journal)
}

fn baseline_recovery_bytes<'a>(journal: &Journal, baseline: &'a crate::bank_backup::Baseline) -> Result<&'a [u8]> {
    anyhow::ensure!(journal.phase == "export-uncertain" && journal.original_sha256.is_none()
        && journal.bundle_suffix == ".pkpass", "This journal is not eligible for baseline recovery");
    anyhow::ensure!(baseline.udid == journal.udid && baseline.card_hash == journal.card_hash,
        "Baseline belongs to another device/card");
    let Some(crate::bank_backup::LocalAssetState::Present { bytes }) = baseline.local_assets.get(&journal.leaf) else {
        bail!("The baseline does not preserve this exact leaf");
    };
    anyhow::ensure!(!bytes.is_empty() && bytes.len() <= resource_limit(&journal.leaf)
        && crate::wallet_backup::CARD_ARTWORK_ASSETS.contains(&journal.leaf.as_str()), "Invalid baseline recovery resource");
    Ok(bytes)
}

fn recovery_baseline(journal: &Journal) -> Result<crate::bank_backup::Baseline> {
    anyhow::ensure!(journal.phase == "export-uncertain" && journal.original_sha256.is_none()
        && journal.bundle_suffix == ".pkpass", "This journal is not eligible for baseline recovery");
    let baseline = crate::bank_backup::load_baseline(&journal.udid, &journal.card_hash)?
        .context("A fully verified baseline is required")?;
    baseline_recovery_bytes(journal, &baseline)?;
    Ok(baseline)
}

fn retain_baseline_for_recovery(operation_dir: &Path, journal: &mut Journal,
    baseline: &crate::bank_backup::Baseline, root: &Path) -> Result<()> {
    validate_operation_dir_at(operation_dir, journal, root)?;
    let bytes = baseline_recovery_bytes(journal, baseline)?;
    let hash = sha256(bytes)?;
    // This is explicitly baseline provenance, never a claim that the failed
    // export observed these bytes. Keep every earlier journal and evidence file.
    let proof = serde_json::to_vec_pretty(&serde_json::json!({
        "version": 1, "source": "verified-bank-baseline", "token": journal.token,
        "udid": journal.udid, "card_hash": journal.card_hash, "leaf": journal.leaf,
        "bundle_suffix": journal.bundle_suffix, "source_revision": journal.revision,
        "sha256": hash, "size": bytes.len(), "phone_verified": false,
        "baseline_manifest_sha256": sha256(&baseline.manifest_bytes)?,
        "baseline_remote_artwork_sha256": sha256(&baseline.remote_artwork_bytes)?
    }))?;
    for (path, data) in [(operation_dir.join("baseline-recovery-provenance.json"), proof.as_slice()),
        (operation_dir.join(format!("{}-baseline.original", journal.leaf)), bytes)] {
        if path.exists() {
            crate::storage::safe_chain(&path)?;
            anyhow::ensure!(fs::symlink_metadata(&path)?.is_file() && fs::read(&path)? == data,
                "Existing recovery evidence differs; nothing was replaced");
        } else { durable_write(&path, data)?; }
    }
    journal.original_sha256 = Some(hash);
    // Remains pending until the existing return/readback/Books pipeline succeeds.
    save_journal(operation_dir, journal)?;
    verified_original_in(operation_dir, journal)?;
    Ok(())
}

/// PC-only explicit preparation. Does not query or write the phone, and does
/// not complete the recovery. The existing return action performs verification.
pub fn prepare_pending_from_baseline(operation_dir: &Path, selected_udid: &str) -> Result<()> {
    let _lease = crate::storage::operation_lease()?;
    let mut journal = latest_journal(operation_dir)?;
    validate_operation_dir(operation_dir, &journal)?;
    anyhow::ensure!(journal.udid.eq_ignore_ascii_case(selected_udid), "Select the matching iPhone");
    let baseline = recovery_baseline(&journal)?;
    retain_baseline_for_recovery(operation_dir, &mut journal, &baseline, &journal_root())
}

fn verified_original_in(operation_dir: &Path, journal: &Journal) -> Result<Vec<u8>> {
    let expected = journal.original_sha256.as_deref().context("No complete PC original is recorded in this phase")?;
    for path in original_candidates(operation_dir, &journal.leaf)? {
        if fs::symlink_metadata(&path).is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
            || fs::symlink_metadata(&path).is_ok_and(|meta| meta.file_type().is_symlink()) { continue; }
        let Ok(bytes) = fs::read(path) else { continue };
        if bytes.len() <= resource_limit(&journal.leaf)
            && sha256(&bytes).is_ok_and(|actual| actual == expected) { return Ok(bytes); }
    }
    bail!("No retained original matches the recovery journal hash")
}

/// Explicitly resend a locally verified original for a pending return phase.
/// Resume uses a fresh protected readback before the final return and marks
/// complete only after a fresh Books snapshot matches the recorded preimage.
pub fn resume_pending_return<L>(operation_dir: &Path, udid: &str, mode: ConnectionMode,
    mut log: L) -> Result<()>
where L: FnMut(&str) {
    let _lease=crate::storage::operation_lease()?;
    let mut journal = latest_journal(operation_dir)?;
    validate_operation_dir(operation_dir, &journal)?;
    anyhow::ensure!(journal.udid.eq_ignore_ascii_case(udid), "Recovery journal belongs to another device");
    let starting_phase = journal.phase.clone();
    if starting_phase == "cleanup-pending" {
        anyhow::ensure!(journal.udid.eq_ignore_ascii_case(udid), "Recovery journal belongs to another device");
        crate::device::ensure_wallet_version_supported(udid)?;
        let _ = load_preserved_original(operation_dir)?;
        let session = ActiveDeviceSession::open(Some(udid), mode)?;
        let afc = AfcClient::new(&session)?;
        anyhow::ensure!(snapshot_books(&afc)?.files.into_iter().collect::<BTreeMap<_, _>>() == journal.books,
            "Books preimage no longer matches; recovery artifacts were retained");
        cleanup_owned_artifacts(udid, mode, &journal)?;
        journal.phase = "complete".into();
        save_journal(operation_dir, &mut journal)?;
        return Ok(());
    }
    anyhow::ensure!(matches!(starting_phase.as_str(), "pc-copy-verified-return-1-pending"
        | "first-return-sync-confirmed" | "readback-pc-copy-verified"
        | "readback-uncertain" | "readback-mismatch-original-preserved"
        | "readback-matched-final-return-pending" | "manual-return-intent" | "export-uncertain")
        || starting_phase.starts_with("resume-readback-"),
        "This recovery phase cannot safely resume automatically; keep the journal and saved original");
    crate::device::ensure_wallet_version_supported(udid)?;
    let original = load_preserved_original(operation_dir)?;
    let target = bundle_target(&journal.card_hash, &journal.bundle_suffix);
    // A failed export may have left its Books manifest installed. A return
    // writer restores its immediate snapshot, so recover the durable preimage
    // first instead of perpetuating that interrupted export request.
    let session = ActiveDeviceSession::open(Some(udid), mode)?;
    let afc = AfcClient::new(&session)?;
    let current = snapshot_books(&afc)?;
    let expected = crate::airlift::BooksSnapshot { files: journal.books.clone().into_iter().collect() };
    if current.files != expected.files {
        durable_write(&operation_dir.join(format!("books-before-recovery-{}.json", crate::flasher::generate_token())),
            &serde_json::to_vec(&current.files)?)?;
        restore_books(&afc, &expected).context("Could not restore the durable Books preimage before return")?;
    }
    anyhow::ensure!(snapshot_books(&afc)?.files == expected.files, "Books preimage recovery failed readback");
    drop(afc);
    drop(session);
    if starting_phase != "readback-matched-final-return-pending" {
        journal.phase = "manual-return-intent".into();
        save_journal(operation_dir, &mut journal)?;
        crate::flasher::write_system_file(udid, mode, &target, &journal.leaf, &original, &mut log)
            .context("Saved original remains on PC; resumed first return is unconfirmed")?;
        journal.phase = "first-return-sync-confirmed".into();
        save_journal(operation_dir, &mut journal)?;
    }
    if starting_phase != "readback-matched-final-return-pending" {
        // The saved original is resent above for every uncertain phase. Verify
        // Books before issuing another export, since an earlier sync can move it.
        let session = ActiveDeviceSession::open(Some(udid), mode)?;
        let afc = AfcClient::new(&session)?;
        anyhow::ensure!(snapshot_books(&afc)?.files.into_iter().collect::<BTreeMap<_, _>>() == journal.books,
            "Books preimage could not be verified before recovery readback");
        let expected_books = journal.books.clone();
        let card_hash = journal.card_hash.clone();
        let leaf = journal.leaf.clone();
        let bundle_suffix = journal.bundle_suffix.clone();
        let (readback, _) = export_round_scoped(udid, mode, &card_hash, &bundle_suffix, &leaf,
            operation_dir, &mut journal, &expected_books, &format!("resume-readback-{}", crate::flasher::generate_token()), &mut log)
            .context("Saved original remains on PC; recovery readback is uncertain")?;
        ensure_readback_matches(&original, &readback)
            .context("Recovery readback differs from the saved original; final return was stopped")?;
        journal.phase = "readback-matched-final-return-pending".into();
        save_journal(operation_dir, &mut journal)?;
    }
    crate::flasher::write_system_file(udid, mode, &target, &journal.leaf, &original, &mut log)
        .context("Saved original and readback match; final recovery return is unconfirmed")?;
    let session = ActiveDeviceSession::open(Some(udid), mode)?;
    let afc = AfcClient::new(&session)?;
    let books: BTreeMap<String, Option<Vec<u8>>> = snapshot_books(&afc)?.files.into_iter().collect();
    anyhow::ensure!(books == journal.books,
        "Books state differs from the preserved preimage; original remains available in the recovery folder");
    journal.phase = "cleanup-pending".into();
    save_journal(operation_dir, &mut journal)?;
    cleanup_owned_artifacts(udid, mode, &journal)?;
    journal.phase = "complete".into();
    save_journal(operation_dir, &mut journal)?;
    Ok(())
}

fn cleanup_owned_artifacts(udid: &str, mode: ConnectionMode, journal: &Journal) -> Result<()> {
    anyhow::ensure!(journal.device_tokens.iter().all(|token| valid_token(token)),
        "Refusing cleanup with an invalid Airlift token");
    let session = ActiveDeviceSession::open(Some(udid), mode)?;
    let afc = AfcClient::new(&session)?;
    for token in &journal.device_tokens {
        let link = format!("{LINK_PREFIX}{token}");
        let recovered = format!("{RECOVERED_PREFIX}{token}");
        let source = format!("{SOURCE_PREFIX}{token}");
        if afc.exists(&link) { afc.remove_path(&link).context("Could not remove owned Airlift link")?; }
        if afc.exists(&recovered) { afc.remove_path(&recovered).context("Could not remove owned recovered slot")?; }
        if afc.exists(&source) { afc.remove_tree(&source).context("Could not remove owned Airlift source")?; }
    }
    Ok(())
}

/// Export and durably preserve one result slot from a single Airlift move.
fn export_round<L>(udid: &str, mode: ConnectionMode, card_hash: &str, leaf: &str,
    operation_dir: &Path, journal: &mut Journal, expected_books: &BTreeMap<String, Option<Vec<u8>>>,
    round: &str, mut log: L) -> Result<(Vec<u8>, BTreeMap<String, Option<Vec<u8>>>)>
where L: FnMut(&str) {
    let suffix = journal.bundle_suffix.clone();
    export_round_scoped(udid, mode, card_hash, &suffix, leaf, operation_dir, journal,
        expected_books, round, &mut log)
}

fn export_round_scoped<L>(udid: &str, mode: ConnectionMode, card_hash: &str, suffix: &str, leaf: &str,
    operation_dir: &Path, journal: &mut Journal, expected_books: &BTreeMap<String, Option<Vec<u8>>>,
    round: &str, mut log: L) -> Result<(Vec<u8>, BTreeMap<String, Option<Vec<u8>>>)>
where L: FnMut(&str) {
    anyhow::ensure!(is_exact_card_hash(card_hash) && valid_suffix_leaf(suffix, leaf),
        "Invalid protected resource target");
    let token = crate::flasher::generate_token();
    let source = format!("{SOURCE_PREFIX}{token}");
    let link = format!("{LINK_PREFIX}{token}");
    let recovered = format!("{RECOVERED_PREFIX}{token}");
    let target = bundle_target(card_hash, suffix);
    let file_ident = airlock_relpath(&format!("{target}/{leaf}"));
    let link_ident = format!("../../{source}/p0/p1/p2/link");
    journal.device_tokens.push(token);
    journal.phase = format!("{round}-export-prepared");
    save_journal(operation_dir, journal).context("Could not journal scoped device paths before export")?;

    let session = ActiveDeviceSession::open(Some(udid), mode).context("Could not open export device session")?;
    let afc = AfcClient::new(&session).context("Could not open AFC for export recovery")?;
    // Never overwrite or adopt a colliding path, even if a random token repeats.
    anyhow::ensure!(!afc.exists(&source) && !afc.exists(&link) && !afc.exists(&recovered),
        "Unique Airlift path already exists; refusing to touch it");
    let snapshot = snapshot_books(&afc).context("Could not snapshot Books before protected export")?;
    let snapshot_matches = snapshot.files.len() == expected_books.len()
        && snapshot.files.iter().all(|(name, bytes)| expected_books.get(name) == Some(bytes));
    anyhow::ensure!(snapshot_matches,
        "Books state differs from the durable preimage; export stopped before mutation");

    let archive = build_streaming_zip_archive(&target, b"wallet-export-v1")?;
    let books = build_books_plist(&[link_ident.clone(), file_ident.clone()])?;
    let export = (|| -> Result<()> {
        stage_streaming_zip(&session, &source, &archive).context("Could not stage protected export")?;
        anyhow::ensure!(afc.exists(&format!("{source}/p0/p1/p2/link")), "Airlift link was not staged");
        afc.make_directory_recursive("Books/Sync")?;
        afc.write_file("Books/Sync/Books.plist", &books)?;
        anyhow::ensure!(afc.exists("Books/Sync/Books.plist"), "Books export request was not staged");
        let requests = [(link_ident.as_str(), link.as_str()), (file_ident.as_str(), recovered.as_str())];
        sync_assets_via_airtraffic(udid, session.transport, &requests, &mut log)
            .context("AirTraffic export did not confirm completion")?;
        Ok(())
    })();
    let limit = resource_limit(leaf);
    if let Some(size) = afc.file_size(&recovered) {
        anyhow::ensure!(size <= limit, "Exported resource exceeds its bounded size; slot retained");
    }
    let data = match afc.read_file(&recovered) {
        Ok(data) if data.len() <= limit => data,
        Ok(_) => bail!("Exported artwork size is invalid; the recovered Media slot was retained"),
        Err(read_error) => match export {
            Err(sync_error) => bail!("AirTraffic result uncertain ({sync_error:#}); export slot unreadable ({read_error:#}); no missing-file conclusion was made"),
            Ok(()) => bail!("Export slot unreadable ({read_error:#}); no missing-file conclusion was made"),
        },
    };
    // Preserve observed bytes before consumers validate an image or manifest.
    let observed_path = operation_dir.join(format!("{leaf}-{round}.observed"));
    durable_write(&observed_path, &data).context("Could not preserve observed export bytes")?;
    // Preserve exact observed bytes, including empty or undecodable resources,
    // before reacting to transport or format state. Consumers apply their own
    // format gates before certifying a backup or generated override.
    let original_path = preserve_exported_copy(operation_dir, journal, leaf, round, &data)?;
    if let Err(error) = export {
        journal.phase = format!("{round}-uncertain-pc-copy-preserved");
        save_journal(operation_dir, journal)?;
        bail!("AirTraffic result uncertain ({error:#}); observed original preserved at {}; Airlift artifacts retained", original_path.display());
    }
    restore_books(&afc, &snapshot).context("Could not restore Books after confirmed export; PC copy and Media slot retained")?;
    anyhow::ensure!(snapshot_books(&afc)?.files == snapshot.files,
        "Books readback after export did not match its preimage; PC original remains preserved");
    Ok((data, snapshot.files.into_iter().collect()))
}

/// Capture only succeeds after export, local verification, return, fresh
/// readback equality, final return, and Books preimage verification.
pub fn capture_verified_original_asset<L>(udid: &str, mode: ConnectionMode,
    card_hash: &str, leaf: &str, mut log: L) -> Result<Vec<u8>>
where L: FnMut(&str) {
    let _lease=crate::storage::operation_lease()?;
    anyhow::ensure!(is_exact_card_hash(card_hash), "Invalid Wallet card identifier");
    anyhow::ensure!(valid_leaf(leaf), "Artwork name must be a single file name");
    crate::device::ensure_wallet_version_supported(udid)?;
    let pending = pending_recovery_operations()?;
    anyhow::ensure!(!pending_recovery_blocks_device(Ok(&pending), Some(udid)),
        "A previous Wallet artwork operation needs recovery review before another export: {}",
        pending.iter().map(|item| format!("{} ({})", item.operation_dir.display(), item.phase)).collect::<Vec<_>>().join(", "));
    let preimage_session = ActiveDeviceSession::open(Some(udid), mode)?;
    let preimage_afc = AfcClient::new(&preimage_session)?;
    let books_before: BTreeMap<String, Option<Vec<u8>>> = snapshot_books(&preimage_afc)?.files.into_iter().collect();
    drop(preimage_afc);
    drop(preimage_session);
    let root = journal_root();
    let operation_dir = create_operation_dir(&root, &crate::flasher::generate_token())?;
    let token = operation_dir.file_name().and_then(|name| name.to_str()).unwrap_or("unknown").to_string();
    let mut journal = Journal { version: 1, revision: 0, token, udid: udid.to_string(),
        card_hash: card_hash.to_string(), leaf: leaf.to_string(), phase: "prepared".into(),
        bundle_suffix: default_bundle_suffix(), original_sha256: None,
        books: books_before.clone().into_iter().collect(), device_tokens: Vec::new() };
    save_journal(&operation_dir, &mut journal).context("Could not durably save Books preimage before export")?;

    let (original, _) = match export_round(udid, mode, card_hash, leaf, &operation_dir, &mut journal, &books_before, "export", &mut log) {
        Ok(result) => result,
        Err(error) => {
            journal.phase = "export-uncertain".into();
            let _ = save_journal(&operation_dir, &mut journal);
            // Recover the recorded auxiliary preimage even when the protected
            // file outcome is uncertain. Preserve every link and recovered slot.
            let auxiliary_recovery = (|| -> Result<()> {
                let session = ActiveDeviceSession::open(Some(udid), mode)?;
                let afc = AfcClient::new(&session)?;
                let current = snapshot_books(&afc)?;
                durable_write(&operation_dir.join("books-after-failed-export.json"),
                    &serde_json::to_vec(&current.files)?)?;
                let preimage = crate::airlift::BooksSnapshot {
                    files: books_before.clone().into_iter().collect() };
                restore_books(&afc, &preimage)?;
                anyhow::ensure!(snapshot_books(&afc)?.files == preimage.files,
                    "Books recovery readback mismatch");
                Ok(())
            })();
            match auxiliary_recovery {
                Ok(()) => log("Books auxiliaries recovered; protected export remains unconfirmed. All recovery artifacts retained."),
                Err(auxiliary_error) => log(&format!("Books recovery is unconfirmed: {auxiliary_error:#}")),
            }
            bail!("Export is uncertain; inspect retained recovery artifacts at {}: {error:#}", operation_dir.display());
        }
    };
    journal.original_sha256 = Some(sha256(&original)?);
    journal.phase = "pc-copy-verified-return-1-pending".into();
    save_journal(&operation_dir, &mut journal)?;

    let target = bundle_target(card_hash, ".pkpass");
    crate::flasher::write_system_file(udid, mode, &target, leaf, &original, &mut log)
        .context("Original is safe on PC; first return was not confirmed")?;
    journal.phase = "first-return-sync-confirmed".into();
    save_journal(&operation_dir, &mut journal)?;

    let (readback, _) = match export_round(udid, mode, card_hash, leaf, &operation_dir, &mut journal, &books_before, "readback", &mut log) {
        Ok(result) => result,
        Err(error) => {
            journal.phase = "readback-uncertain".into();
            let _ = save_journal(&operation_dir, &mut journal);
            bail!("Original is safe on PC; readback is uncertain, recovery artifacts retained at {}: {error:#}", operation_dir.display());
        }
    };
    if let Err(error) = ensure_readback_matches(&original, &readback) {
        journal.phase = "readback-mismatch-original-preserved".into();
        save_journal(&operation_dir, &mut journal)?;
        return Err(error.context(format!("Final return skipped; recovery artifacts retained at {}", operation_dir.display())));
    }
    journal.phase = "readback-matched-final-return-pending".into();
    save_journal(&operation_dir, &mut journal)?;

    crate::flasher::write_system_file(udid, mode, &target, leaf, &original, &mut log)
        .context("PC original and readback match; final return was not confirmed")?;
    let verify_session = ActiveDeviceSession::open(Some(udid), mode)?;
    let verify_afc = AfcClient::new(&verify_session)?;
    let final_books: BTreeMap<String, Option<Vec<u8>>> = snapshot_books(&verify_afc)?.files.into_iter().collect();
    anyhow::ensure!(final_books == books_before,
        "Books state differs from its preimage after final return; recovery artifacts retained at {}", operation_dir.display());
    drop(verify_afc);
    drop(verify_session);
    journal.phase = "cleanup-pending".into();
    save_journal(&operation_dir, &mut journal)?;
    cleanup_owned_artifacts(udid, mode, &journal)?;
    journal.phase = "complete".into();
    save_journal(&operation_dir, &mut journal)?;
    log(&format!("Original saved and read back on this PC; both returns were confirmed and Books state verified. Recovery record: {}", operation_dir.display()));
    Ok(original)
}

/// Inspect exact override leaves through a relocated link without moving any
/// protected file. `None` means AFC explicitly returned OBJECT_NOT_FOUND after
/// the link, parent directory and remote manifest were independently checked.
pub fn inspect_bank_overrides<L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    returned_manifest: &[u8], mut log: L) -> Result<BTreeMap<String, MetadataState>>
where L: FnMut(&str) {
    let _lease=crate::storage::operation_lease()?;
    inspect_bank_overrides_inner(udid, mode, card_hash, returned_manifest, None, &mut log)
}

fn inspect_bank_overrides_inner<L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    returned_manifest: &[u8], allowed_pending: Option<&Path>, mut log: L)
    -> Result<BTreeMap<String, MetadataState>>
where L: FnMut(&str) {
    anyhow::ensure!(is_exact_card_hash(card_hash), "Invalid card identifier");
    // Caller must obtain this from capture_verified_original_asset for this
    // exact device/card immediately before inspection. Metadata code 10 alone
    // cannot establish that a manifest exists or that any override is missing.
    anyhow::ensure!(valid_resource(BANK_MANIFEST, returned_manifest), "No verified returned bank manifest supplied");
    crate::device::ensure_wallet_version_supported(udid)?;
    let pending = pending_recovery_operations()?;
    anyhow::ensure!(!pending_recovery_blocks_device_except(Ok(&pending), Some(udid), allowed_pending),
        "Pending recovery must finish before inspection");
    let session = ActiveDeviceSession::open(Some(udid), mode)?;
    let afc = AfcClient::new(&session)?;
    let snapshot = snapshot_books(&afc)?;
    let token = crate::flasher::generate_token();
    let operation_dir = create_operation_dir(&journal_root(), &token)?;
    let mut journal = Journal { version: 1, revision: 0, token: token.clone(),
        udid: udid.into(), card_hash: card_hash.into(), leaf: BANK_MANIFEST.into(),
        bundle_suffix: default_bundle_suffix(), phase: "metadata-prepared".into(), original_sha256: None,
        books: snapshot.files.clone().into_iter().collect(), device_tokens: vec![token.clone()] };
    save_journal(&operation_dir, &mut journal)?;
    let source = format!("{SOURCE_PREFIX}{token}");
    let link = format!("{LINK_PREFIX}{token}");
    let target = bundle_target(card_hash, ".pkpass");
    let identifier = format!("../../{source}/p0/p1/p2/link");
    let outcome = (|| -> Result<BTreeMap<String, MetadataState>> {
        anyhow::ensure!(!afc.exists(&source) && !afc.exists(&link), "Own metadata paths collided");
        let archive = build_streaming_zip_archive(&target, b"metadata-only")?;
        stage_streaming_zip(&session, &source, &archive)?;
        afc.make_directory_recursive("Books/Sync")?;
        afc.write_file("Books/Sync/Books.plist", &build_books_plist(&[identifier.clone()])?)?;
        sync_assets_via_airtraffic(udid, session.transport, &[(identifier.as_str(), link.as_str())], &mut log)?;
        let (link_status, link_info) = afc.file_info_status(&link);
        anyhow::ensure!(link_status == 0 && link_info.get("st_ifmt").is_some_and(|v| v == "S_IFLNK")
            && link_info.get("LinkTarget").is_some_and(|v| v == &format!("../../..{target}")),
            "Relocated metadata link target is unverified (status {link_status}, keys {:?})", link_info.keys().collect::<Vec<_>>());
        let (parent_status, parent) = afc.file_info_status(&format!("{link}/"));
        anyhow::ensure!(parent_status == 0 && parent.get("st_ifmt").is_some_and(|v| v == "S_IFDIR"),
            "Protected parent is unverified (AFC {parent_status}); absence cannot be certified");
        let (manifest_status, manifest) = afc.file_info_status(&format!("{link}/{BANK_MANIFEST}"));
        anyhow::ensure!(manifest_status == 10 || (manifest_status == 0 && manifest.get("st_ifmt").is_some_and(|v| v == "S_IFREG")
            && manifest.get("st_size").and_then(|v| v.parse::<usize>().ok()) == Some(returned_manifest.len())),
            "Known bank manifest is unverified (AFC {manifest_status}); absence cannot be certified");
        let mut states = BTreeMap::new();
        for leaf in crate::wallet_backup::CARD_ARTWORK_ASSETS {
            let (status, info) = afc.file_info_status(&format!("{link}/{leaf}"));
            let state = match status {
                8 => MetadataState::Absent,
                10 => MetadataState::Unreadable,
                0 => {
                    anyhow::ensure!(info.get("st_ifmt").is_some_and(|v| v == "S_IFREG"), "Override is not a regular file");
                    let size = info.get("st_size").and_then(|v| v.parse::<usize>().ok()).context("Override size unknown")?;
                    anyhow::ensure!(size > 0 && size <= MAX_ARTWORK_BYTES, "Override size out of bounds");
                    MetadataState::Present { size }
                }
                _ => bail!("Override metadata is uncertain (AFC {status}); no absence was inferred"),
            };
            log(&format!("BANK_OVERRIDE: {leaf}; AFC={status}; state={state:?}"));
            states.insert(leaf.into(), state);
        }
        durable_write(&operation_dir.join("override-metadata.json"), &serde_json::to_vec_pretty(&states)?)?;
        Ok(states)
    })();
    let recovery = restore_books(&afc, &snapshot).and_then(|()| {
        anyhow::ensure!(snapshot_books(&afc)?.files == snapshot.files, "Books metadata preimage mismatch");
        Ok(())
    });
    if let Err(error) = recovery {
        journal.phase = "metadata-auxiliary-recovery-needed".into();
        save_journal(&operation_dir, &mut journal)?;
        bail!("Metadata Books recovery unconfirmed: {error:#}; artifacts retained");
    }
    // This operation never requested movement of a protected resource. After
    // verified auxiliary recovery, only the scoped link/staging tree is removed.
    cleanup_owned_artifacts(udid, mode, &journal)?;
    journal.phase = "complete".into();
    save_journal(&operation_dir, &mut journal)?;
    outcome
}

/// Inspect one allow-listed leaf through a relocated link. Unlike a direct AFC
/// absolute-path query, only a verified link and readable target directory can
/// establish absence for the leaf.
fn inspect_scoped_leaf<L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    suffix: &str, leaf: &str, allowed_pending: Option<&Path>, mut log: L)
    -> Result<MetadataState>
where L: FnMut(&str) {
    anyhow::ensure!(is_exact_card_hash(card_hash) && valid_suffix_leaf(suffix, leaf),
        "Invalid scoped AFC inspection target");
    crate::device::ensure_wallet_version_supported(udid)?;
    let pending = pending_recovery_operations()?;
    anyhow::ensure!(!pending_recovery_blocks_device_except(Ok(&pending), Some(udid), allowed_pending),
        "Pending recovery must finish before scoped AFC inspection");
    let session = ActiveDeviceSession::open(Some(udid), mode)?;
    let afc = AfcClient::new(&session)?;
    let snapshot = snapshot_books(&afc)?;
    let token = crate::flasher::generate_token();
    let operation_dir = create_operation_dir(&journal_root(), &token)?;
    let mut journal = Journal { version: 1, revision: 0, token: token.clone(), udid: udid.into(),
        card_hash: card_hash.into(), leaf: leaf.into(), bundle_suffix: suffix.into(),
        phase: "scoped-metadata-prepared".into(), original_sha256: None,
        books: snapshot.files.clone().into_iter().collect(), device_tokens: vec![token.clone()] };
    save_journal(&operation_dir, &mut journal)?;
    let source = format!("{SOURCE_PREFIX}{token}");
    let link = format!("{LINK_PREFIX}{token}");
    let target = bundle_target(card_hash, suffix);
    let link_ident = format!("../../{source}/p0/p1/p2/link");
    let outcome = (|| -> Result<MetadataState> {
        anyhow::ensure!(!afc.exists(&source) && !afc.exists(&link), "Scoped metadata path collision");
        stage_streaming_zip(&session, &source, &build_streaming_zip_archive(&target, b"scoped-metadata")?)?;
        afc.make_directory_recursive("Books/Sync")?;
        afc.write_file("Books/Sync/Books.plist", &build_books_plist(&[link_ident.clone()])?)?;
        sync_assets_via_airtraffic(udid, session.transport, &[(link_ident.as_str(), link.as_str())], &mut log)?;
        let expected_target = format!("../../..{target}");
        let (link_status, link_info) = afc.file_info_status(&link);
        anyhow::ensure!(link_status == 0 && link_info.get("st_ifmt").is_some_and(|kind| kind == "S_IFLNK")
            && link_info.get("LinkTarget").is_some_and(|target| target == &expected_target),
            "Scoped link target was not verified (AFC {link_status})");
        let (parent_status, parent) = afc.file_info_status(&format!("{link}/"));
        if parent_status == 8 {
            // The link target was verified above. OBJECT_NOT_FOUND on the
            // target directory means this exact cache directory is absent.
            log("Verified relocated cache target directory is absent (AFC 8).");
            return Ok(MetadataState::Absent);
        }
        anyhow::ensure!(parent_status == 0 && parent.get("st_ifmt").is_some_and(|kind| kind == "S_IFDIR"),
            "Scoped parent is not verified (AFC {parent_status}); absence cannot be certified");
        let (status, info) = afc.file_info_status(&format!("{link}/{leaf}"));
        let state = match status {
            8 => MetadataState::Absent,
            10 => MetadataState::Unreadable,
            0 => {
                anyhow::ensure!(info.get("st_ifmt").is_some_and(|kind| kind == "S_IFREG"),
                    "Scoped leaf is not a regular file");
                let size = info.get("st_size").and_then(|value| value.parse::<usize>().ok())
                    .context("Scoped leaf size is unknown")?;
                anyhow::ensure!(size <= resource_limit(leaf), "Scoped leaf exceeds read limit");
                MetadataState::Present { size }
            }
            _ => bail!("Scoped leaf metadata is uncertain (AFC {status})"),
        };
        durable_write(&operation_dir.join("scoped-metadata.json"), &serde_json::to_vec(&state)?)?;
        Ok(state)
    })();
    let recovery = restore_books(&afc, &snapshot).and_then(|()| {
        anyhow::ensure!(snapshot_books(&afc)?.files == snapshot.files, "Books scoped metadata preimage mismatch");
        Ok(())
    });
    if let Err(error) = recovery {
        journal.phase = "metadata-auxiliary-recovery-needed".into();
        save_journal(&operation_dir, &mut journal)?;
        bail!("Scoped metadata Books recovery unconfirmed: {error:#}; evidence retained");
    }
    cleanup_owned_artifacts(udid, mode, &journal)?;
    journal.phase = "complete".into();
    save_journal(&operation_dir, &mut journal)?;
    outcome
}

/// Read-only diagnostic for one allow-listed cache leaf. Books metadata is
/// restored and the temporary link is removed before this returns.
pub fn inspect_cache_leaf<L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    suffix: &str, leaf: &str, log: L) -> Result<MetadataState>
where L: FnMut(&str) {
    anyhow::ensure!(matches!(suffix, ".cache" | ".pkcache") && valid_suffix_leaf(suffix, leaf),
        "Invalid cache inspection target");
    inspect_scoped_leaf(udid, mode, card_hash, suffix, leaf, None, log)
}

/// Move one known cache leaf out of Wallet, preserving exact bytes in its own
/// bounded export journal. Only a verified post-move AFC absence completes the
/// operation; present/denied outcomes retain the Media slot and recovery data.
pub fn remove_verified_cache_leaf<L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    suffix: &str, leaf: &str, mut log: L) -> Result<Option<String>>
where L: FnMut(&str) {
    anyhow::ensure!(matches!(suffix, ".cache" | ".pkcache") && valid_suffix_leaf(suffix, leaf),
        "Invalid cache removal target");
    anyhow::ensure!(!pending_recovery_blocks_device(Ok(&pending_recovery_operations()?), Some(udid)),
        "A prior export needs recovery before cache invalidation");
    match inspect_scoped_leaf(udid, mode, card_hash, suffix, leaf, None, &mut log)? {
        MetadataState::Absent => {
            log(&format!("Cache {suffix}/{leaf} is already absent under a verified parent."));
            return Ok(None);
        }
        MetadataState::Unreadable => log(&format!(
            "AFC denied metadata for {suffix}/{leaf}; attempting a journaled export, without inferring absence.")),
        MetadataState::Present { .. } => {}
    }

    let session = ActiveDeviceSession::open(Some(udid), mode)?;
    let afc = AfcClient::new(&session)?;
    let books = snapshot_books(&afc)?;
    let token = crate::flasher::generate_token();
    let operation_dir = create_operation_dir(&journal_root(), &token)?;
    let mut journal = Journal { version: 1, revision: 0, token: token.clone(), udid: udid.into(),
        card_hash: card_hash.into(), leaf: leaf.into(), bundle_suffix: suffix.into(),
        phase: "cache-remove-prepared".into(), original_sha256: None,
        books: books.files.into_iter().collect(), device_tokens: Vec::new() };
    save_journal(&operation_dir, &mut journal)?;
    let expected_books = journal.books.clone();
    if let Err(error) = export_round_scoped(udid, mode, card_hash, suffix, leaf, &operation_dir,
        &mut journal, &expected_books, "cache-remove", &mut log) {
        journal.phase = "cache-remove-uncertain".into();
        let _ = save_journal(&operation_dir, &mut journal);
        bail!("Cache move uncertain; exact PC evidence and device slot retained at {}: {error:#}", operation_dir.display());
    }
    journal.phase = "cache-postcondition-check".into();
    save_journal(&operation_dir, &mut journal)?;
    match inspect_scoped_leaf(udid, mode, card_hash, suffix, leaf, Some(&operation_dir), &mut log) {
        Ok(MetadataState::Absent) => {
            let original_sha256 = journal.original_sha256.as_deref()
                .context("Removed cache copy lacks its exact hash")?;
            durable_write(&operation_dir.join("cache-absence-proof.json"),
                &serde_json::to_vec_pretty(&serde_json::json!({
                    "version": 1,
                    "bundle_suffix": suffix,
                    "leaf": leaf,
                    "original_sha256": original_sha256,
                    "absence": "verified-through-relocated-link"
                }))?)?;
            cleanup_owned_artifacts(udid, mode, &journal)?;
            journal.phase = "complete".into();
            save_journal(&operation_dir, &mut journal)?;
            log(&format!("Cache {suffix}/{leaf} removed with exact PC copy and verified AFC absence. Recovery token: {token}. Restore with: Wallet Studio.exe --wallet-studio-restore-cache {token}"));
            Ok(Some(token))
        }
        Ok(state) => {
            journal.phase = "cache-postcondition-uncertain".into();
            save_journal(&operation_dir, &mut journal)?;
            bail!("Cache {suffix}/{leaf} postcondition is {state:?}; moved bytes and recovery evidence retained at {}", operation_dir.display())
        }
        Err(error) => {
            journal.phase = "cache-postcondition-uncertain".into();
            save_journal(&operation_dir, &mut journal)?;
            bail!("Cache {suffix}/{leaf} absence could not be verified: {error:#}; moved bytes and recovery evidence retained at {}", operation_dir.display())
        }
    }
}

/// Restore one completed cache removal only when the exact protected leaf is
/// still absent. The journal's normal recovery protocol verifies the returned
/// bytes and Books preimage before completing.
pub fn restore_verified_cache_leaf<L>(operation_dir: &Path, udid: &str, mode: ConnectionMode,
    mut log: L) -> Result<()>
where L: FnMut(&str) {
    let mut journal = latest_journal(operation_dir)?;
    validate_operation_dir(operation_dir, &journal)?;
    anyhow::ensure!(journal.phase == "complete" && matches!(journal.bundle_suffix.as_str(), ".cache" | ".pkcache")
        && CACHE_LEAVES.contains(&journal.leaf.as_str()) && journal.udid.eq_ignore_ascii_case(udid),
        "Cache recovery journal is incomplete or belongs to another device");
    let _ = verified_original_in(operation_dir, &journal)?;
    let proof_path = operation_dir.join("cache-absence-proof.json");
    let proof_meta = fs::symlink_metadata(&proof_path).context("Cache removal has no verified-absence proof")?;
    anyhow::ensure!(proof_meta.is_file() && !proof_meta.file_type().is_symlink() && proof_meta.len() <= 16 * 1024,
        "Cache absence proof is unsafe");
    let proof: serde_json::Value = serde_json::from_slice(&fs::read(&proof_path)?)?;
    anyhow::ensure!(proof.get("version").and_then(|value| value.as_u64()) == Some(1)
        && proof.get("bundle_suffix").and_then(|value| value.as_str()) == Some(journal.bundle_suffix.as_str())
        && proof.get("leaf").and_then(|value| value.as_str()) == Some(journal.leaf.as_str())
        && proof.get("original_sha256").and_then(|value| value.as_str()) == journal.original_sha256.as_deref()
        && proof.get("absence").and_then(|value| value.as_str()) == Some("verified-through-relocated-link"),
        "Cache absence proof does not match this recovery journal");
    anyhow::ensure!(matches!(inspect_scoped_leaf(udid, mode, &journal.card_hash, &journal.bundle_suffix,
        &journal.leaf, None, &mut log)?, MetadataState::Absent),
        "Cache leaf is no longer absent; refusing to overwrite current contents");
    journal.phase = "manual-return-intent".into();
    save_journal(operation_dir, &mut journal)?;
    resume_pending_return(operation_dir, udid, mode, &mut log)
}

pub fn restore_cache_leaf_by_token<L>(token: &str, udid: &str, mode: ConnectionMode,
    log: L) -> Result<()>
where L: FnMut(&str) {
    anyhow::ensure!(valid_token(token), "Invalid cache recovery token");
    restore_verified_cache_leaf(&journal_root().join(token), udid, mode, log)
}

/// Remove one baseline-absent bank override only when the active, durable
/// Wallet Studio intent proves the exact bytes being moved are ours. The move
/// result is accepted only after AFC 8 on a relocated link with a verified
/// parent and captured manifest. Uncertain or foreign bytes are returned via
/// the journal's normal recovery path.
pub fn remove_verified_override<L>(udid: &str, mode: ConnectionMode, card_hash: &str,
    leaf: &str, expected_own_bytes: &[u8], mut log: L) -> Result<()>
where L: FnMut(&str) {
    anyhow::ensure!(is_exact_card_hash(card_hash) && crate::wallet_backup::CARD_ARTWORK_ASSETS.contains(&leaf),
        "Invalid bank override removal target");
    let baseline = crate::bank_backup::load_baseline(udid, card_hash)?
        .context("A verified bank baseline is required before removing an override")?;
    anyhow::ensure!(matches!(baseline.local_assets.get(leaf), Some(crate::bank_backup::LocalAssetState::Absent)),
        "Refusing removal: this leaf was not proven absent in the original bank baseline");
    let intent = crate::bank_changes::load_latest_intent(udid, card_hash)?
        .context("Refusing removal without a durable bank write intent")?;
    anyhow::ensure!(intent.phase != "restored" && intent.assets.get(leaf).is_some_and(|bytes| bytes == expected_own_bytes),
        "Refusing removal: target bytes are not bound to the active Wallet Studio write intent");
    remove_authorized_override(udid,mode,card_hash,leaf,&baseline,|bytes|Ok(bytes==expected_own_bytes),&mut log)
}
pub(crate) fn remove_cloud_owned_override<L>(udid:&str,mode:ConnectionMode,card_hash:&str,
    leaf:&str,plan:&crate::bank_recovery::RecoveryPlan,mut log:L)->Result<()> where L:FnMut(&str) {
    plan.revalidate()?;let baseline=plan.baseline();
    anyhow::ensure!(baseline.udid==udid && baseline.card_hash==card_hash
        && crate::wallet_backup::CARD_ARTWORK_ASSETS.contains(&leaf)
        && matches!(baseline.local_assets.get(leaf),Some(crate::bank_backup::LocalAssetState::Absent)),"cloud recovery target was not originally absent");
    remove_authorized_override(udid,mode,card_hash,leaf,baseline,|bytes|plan.owns(udid,card_hash,leaf,bytes),&mut log)
}
fn remove_authorized_override<L>(udid:&str,mode:ConnectionMode,card_hash:&str,leaf:&str,
    baseline:&crate::bank_backup::Baseline,owns:impl Fn(&[u8])->Result<bool>,mut log:L)->Result<()> where L:FnMut(&str) {
    crate::device::ensure_wallet_version_supported(udid)?;
    anyhow::ensure!(!pending_recovery_blocks_device(Ok(&pending_recovery_operations()?), Some(udid)),
        "A prior export needs recovery before removing a bank override");

    let current = inspect_bank_overrides_inner(udid, mode, card_hash, &baseline.manifest_bytes, None, &mut log)?;
    match current.get(leaf).context("Bank metadata omitted the requested leaf")? {
        MetadataState::Absent => {
            log(&format!("Verified original absence for {leaf}; no device file was removed."));
            return Ok(());
        }
        MetadataState::Unreadable => log(&format!(
            "AFC denied metadata for {leaf}; exporting with a recovery journal to compare exact owned bytes.")),
        MetadataState::Present { .. } => {}
    }

    let session = ActiveDeviceSession::open(Some(udid), mode)?;
    let afc = AfcClient::new(&session)?;
    let books = snapshot_books(&afc)?;
    let token = crate::flasher::generate_token();
    let operation_dir = create_operation_dir(&journal_root(), &token)?;
    let mut journal = Journal { version: 1, revision: 0, token: token.clone(), udid: udid.into(),
        card_hash: card_hash.into(), bundle_suffix: default_bundle_suffix(), leaf: leaf.into(),
        phase: "remove-prepared".into(), original_sha256: None,
        books: books.files.clone().into_iter().collect(), device_tokens: Vec::new() };
    save_journal(&operation_dir, &mut journal)?;
    let expected_books = journal.books.clone();
    let observed = export_round_scoped(udid, mode, card_hash, ".pkpass", leaf, &operation_dir,
        &mut journal, &expected_books, "remove", &mut log)
        .map(|(bytes, _)| bytes);
    let bytes = match observed {
        Ok(bytes) => bytes,
        Err(error) => {
            journal.phase = "manual-return-intent".into();
            let _ = save_journal(&operation_dir, &mut journal);
            bail!("Removal/export outcome is uncertain; saved evidence remains at {}: {error:#}", operation_dir.display());
        }
    };
    if !owns(&bytes).unwrap_or(false) {
        journal.phase = "manual-return-intent".into();
        save_journal(&operation_dir, &mut journal)?;
        resume_pending_return(&operation_dir, udid, mode, &mut log)
            .context("The moved content did not match our intent; exact recovery return is unconfirmed")?;
        bail!("Refused removal: the bytes did not match the active design. Original bytes were returned and the intent remains open.");
    }

    // If interrupted during the following metadata probe, the generic recovery
    // UI can safely put these exact bytes back before any retry.
    journal.phase = "manual-return-intent".into();
    save_journal(&operation_dir, &mut journal)?;
    let states = inspect_bank_overrides_inner(udid, mode, card_hash, &baseline.manifest_bytes,
        Some(&operation_dir), &mut log)?;
    let state = states.get(leaf).context("Post-move inspection omitted the requested leaf")?;
    if classify_removal_postcondition(state) == RemovalPostcondition::PreserveEvidence {
        // The moved Media copy is the only guaranteed copy after an uncertain
        // postcondition. Keep its bytes, link/token and manual intent journal;
        // the generic recovery action is intentionally disabled for this
        // state so it cannot overwrite a file that metadata says is present.
        journal.phase = "remove-postcondition-uncertain".into();
        save_journal(&operation_dir, &mut journal)?;
        bail!("No verified absence for {leaf} ({state:?}). The moved bytes and recovery evidence remain on the device and PC; do not retry automatically.")
    }
    cleanup_owned_artifacts(udid, mode, &journal)?;
    journal.phase = "complete".into();
    save_journal(&operation_dir, &mut journal)?;
    log(&format!("Verified removal of Wallet Studio's exact override {leaf}; original baseline was Absent."));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recovery_fixture(journal: &Journal) -> crate::bank_backup::Baseline {
        crate::bank_backup::Baseline { udid: journal.udid.clone(), card_hash: journal.card_hash.clone(),
            manifest_bytes: b"fixture manifest".to_vec(), artwork_name: "fixture".into(),
            remote_artwork_bytes: b"fixture issuer artwork".to_vec(), remote_size: 22, remote_sha1: String::new(),
            local_assets: [(journal.leaf.clone(), crate::bank_backup::LocalAssetState::Present {
                bytes: b"independently verified original fixture".to_vec() })].into_iter().collect() }
    }

    #[test]
    fn baseline_recovery_retains_audit_and_stays_pending_until_phone_verification() {
        let root = std::env::temp_dir().join(format!("baseline-recovery-{}", crate::flasher::generate_token()));
        let mut journal = test_journal(); journal.phase = "export-uncertain".into();
        let dir = create_operation_dir(&root, &journal.token).unwrap();
        save_journal(&dir, &mut journal).unwrap();
        let old_path = dir.join("state-0001-export-uncertain.json"); let old = fs::read(&old_path).unwrap();
        let baseline = recovery_fixture(&journal);
        retain_baseline_for_recovery(&dir, &mut journal, &baseline, &root).unwrap();
        assert_eq!(fs::read(old_path).unwrap(), old);
        assert_eq!(latest_journal(&dir).unwrap().phase, "export-uncertain");
        assert_eq!(verified_original_in(&dir, &latest_journal(&dir).unwrap()).unwrap(), b"independently verified original fixture");
        let proof: serde_json::Value = serde_json::from_slice(&fs::read(dir.join("baseline-recovery-provenance.json")).unwrap()).unwrap();
        assert_eq!(proof["source"], "verified-bank-baseline"); assert_eq!(proof["phone_verified"], false);
        assert!(retain_baseline_for_recovery(&dir, &mut journal, &baseline, &root).is_err());
        assert_eq!(pending_recovery_operations_at(&root).unwrap().len(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn baseline_recovery_rejects_wrong_identity_absence_phase_and_existing_hash() {
        let mut journal = test_journal(); journal.phase = "export-uncertain".into();
        let mut baseline = recovery_fixture(&journal);
        baseline.udid = "another phone".into(); assert!(baseline_recovery_bytes(&journal, &baseline).is_err());
        baseline.udid = journal.udid.clone(); baseline.card_hash = "another card".into();
        assert!(baseline_recovery_bytes(&journal, &baseline).is_err());
        baseline = recovery_fixture(&journal);
        baseline.local_assets.insert(journal.leaf.clone(), crate::bank_backup::LocalAssetState::Absent);
        assert!(baseline_recovery_bytes(&journal, &baseline).is_err());
        baseline = recovery_fixture(&journal); journal.bundle_suffix = ".cache".into();
        assert!(baseline_recovery_bytes(&journal, &baseline).is_err());
        journal.bundle_suffix = ".pkpass".into(); journal.phase = "complete".into();
        assert!(baseline_recovery_bytes(&journal, &baseline).is_err());
        journal.phase = "export-uncertain".into(); journal.original_sha256 = Some("a".repeat(64));
        assert!(baseline_recovery_bytes(&journal, &baseline).is_err());
    }

    #[test]
    fn baseline_recovery_never_replaces_conflicting_retained_evidence() {
        let root = std::env::temp_dir().join(format!("baseline-conflict-{}", crate::flasher::generate_token()));
        let mut journal = test_journal(); journal.phase = "export-uncertain".into();
        let dir = create_operation_dir(&root, &journal.token).unwrap(); save_journal(&dir, &mut journal).unwrap();
        let path = dir.join(format!("{}-baseline.original", journal.leaf)); durable_write(&path, b"different evidence").unwrap();
        let baseline = recovery_fixture(&journal);
        assert!(retain_baseline_for_recovery(&dir, &mut journal, &baseline, &root).is_err());
        assert_eq!(fs::read(path).unwrap(), b"different evidence");
        assert!(latest_journal(&dir).unwrap().original_sha256.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    fn test_journal() -> Journal {
        Journal { version: 1, revision: 0, token: "abcdef1234567890abcd".into(),
            udid: "device".into(), card_hash: "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=".into(),
            leaf: "cardBackgroundCombined@3x.png".into(), bundle_suffix: default_bundle_suffix(), phase: "prepared".into(),
            original_sha256: None,
            books: crate::airlift::TRACKED_BOOKS_FILES.iter().map(|path| (path.to_string(), None)).collect(),
            device_tokens: Vec::new() }
    }

    #[test]
    fn post_move_presence_or_denial_retains_removed_bytes_and_is_not_success() {
        assert_eq!(classify_removal_postcondition(&MetadataState::Absent), RemovalPostcondition::ConfirmedAbsent);
        assert_eq!(classify_removal_postcondition(&MetadataState::Present { size: 12 }), RemovalPostcondition::PreserveEvidence);
        assert_eq!(classify_removal_postcondition(&MetadataState::Unreadable), RemovalPostcondition::PreserveEvidence);
    }

    #[test]
    fn first_operation_creates_missing_journal_root() {
        let root = std::env::temp_dir().join(format!("wallet-export-root-test-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let operation = create_operation_dir(&root, "abcdef1234567890abcd").unwrap();
        assert!(operation.is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_hash_and_artwork_leaf_gates_reject_normalized_inputs() {
        assert!(is_exact_card_hash("OM6NYhwXMZrAw0sRUjR62wmF4ZQ="));
        assert!(!is_exact_card_hash(" OM6NYhwXMZrAw0sRUjR62wmF4ZQ="));
        assert!(!is_exact_card_hash("'OM6NYhwXMZrAw0sRUjR62wmF4ZQ='"));
        assert!(!is_exact_card_hash("OM6NYhwXMZrAw0sRUjR62wmF4ZQ=,"));
        assert!(valid_leaf("cardBackgroundCombined@3x.png"));
        assert!(!valid_leaf("anything.png"));
    }

    #[test]
    fn export_leaf_must_be_a_single_path_component() {
        assert!(valid_leaf("cardBackgroundCombined@3x.png"));
        assert!(!valid_leaf(""));
        assert!(!valid_leaf("../outside"));
        assert!(!valid_leaf("folder/file.png"));
        assert!(!valid_leaf("folder\\file.png"));
    }

    #[test]
    fn bundle_suffix_is_scoped_and_legacy_journals_default_to_pkpass() {
        assert!(valid_suffix_leaf(".pkpass", "cardBackgroundCombined@3x.png"));
        assert!(valid_suffix_leaf(".cache", "FrontFace"));
        assert!(valid_suffix_leaf(".pkcache", "Preview"));
        assert!(!valid_suffix_leaf(".cache", "cardBackgroundCombined@3x.png"));
        assert!(!valid_suffix_leaf(".pkpass", "FrontFace"));
        assert!(!valid_suffix_leaf(".other", "FrontFace"));

        let mut legacy = serde_json::to_value(test_journal()).unwrap();
        legacy.as_object_mut().unwrap().remove("bundle_suffix");
        let decoded: Journal = serde_json::from_value(legacy).unwrap();
        assert_eq!(decoded.bundle_suffix, ".pkpass");
    }

    #[test]
    fn protected_asset_ident_is_relative_to_var_mobile_once() {
        assert_eq!(airlock_relpath("/var/mobile/Library/Passes/Cards/card.pkpass/front.png"),
            "../../../Library/Passes/Cards/card.pkpass/front.png");
    }

    #[test]
    fn durable_evidence_is_read_back_and_never_overwritten() {
        let path = std::env::temp_dir().join(format!("wallet-export-test-{}-{}.json",
            std::process::id(), crate::flasher::generate_token()));
        let bytes = b"durable evidence";
        durable_write(&path, bytes).unwrap();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(durable_write(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn journal_states_are_immutable_and_latest_revision_is_recoverable() {
        let dir = std::env::temp_dir().join(format!("wallet-export-journal-test-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let mut journal = test_journal();
        save_journal(&dir, &mut journal).unwrap();
        journal.phase = "pc-copy-verified".into();
        save_journal(&dir, &mut journal).unwrap();
        assert_eq!(latest_journal(&dir).unwrap().phase, "pc-copy-verified");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn uncertain_transport_keeps_a_hash_checked_pc_original_and_journal() {
        let dir = std::env::temp_dir().join(format!("wallet-export-uncertain-test-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let mut journal = test_journal();
        journal.device_tokens = vec!["abcdef123".into()];
        save_journal(&dir, &mut journal).unwrap();
        let observed = b"export observed despite timeout";
        let path = preserve_uncertain_export(&dir, &mut journal, "cardBackgroundCombined@3x.png", "export", observed).unwrap();
        assert_eq!(fs::read(path).unwrap(), observed);
        assert_eq!(latest_journal(&dir).unwrap().phase, "export-uncertain-pc-copy-preserved");
        assert_eq!(journal.original_sha256.as_deref(), Some(sha256(observed).unwrap().as_str()));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn later_mismatching_readback_cannot_replace_original_recovery_hash() {
        let dir = std::env::temp_dir().join(format!("wallet-export-hash-test-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let mut journal = test_journal();
        let mut original = Vec::new();
        image::DynamicImage::new_rgb8(2, 2).write_to(&mut std::io::Cursor::new(&mut original), image::ImageFormat::Png).unwrap();
        let mut mismatch = Vec::new();
        image::DynamicImage::new_rgb8(3, 3).write_to(&mut std::io::Cursor::new(&mut mismatch), image::ImageFormat::Png).unwrap();
        let leaf = journal.leaf.clone();
        preserve_exported_copy(&dir, &mut journal, &leaf, "export", &original).unwrap();
        let original_hash = journal.original_sha256.clone();
        preserve_exported_copy(&dir, &mut journal, &leaf, "readback", &mismatch).unwrap();
        assert_eq!(journal.original_sha256, original_hash);
        assert!(ensure_readback_matches(&original, &mismatch).is_err());
        save_journal(&dir, &mut journal).unwrap();
        assert_eq!(verified_original_in(&dir, &journal).unwrap(), original);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn invalid_journal_tokens_books_paths_and_hashes_are_rejected() {
        let mut journal = test_journal();
        journal.device_tokens.push("../outside".into());
        assert!(validate_journal(&journal).is_err());
        journal.device_tokens.clear();
        journal.books.clear();
        assert!(validate_journal(&journal).is_err());
        journal.books = crate::airlift::TRACKED_BOOKS_FILES.iter().map(|path| (path.to_string(), None)).collect();
        journal.original_sha256 = Some("not-a-hash".into());
        assert!(validate_journal(&journal).is_err());
        journal.original_sha256 = None;
        journal.leaf = "../outside".into();
        assert!(validate_journal(&journal).is_err());
    }
}
