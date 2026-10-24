//! Durable write-intent records for the exact three local bank-card overrides.
//! Native callers must create the intent before any device write and advance
//! phases only after their own transfer/return verification succeeds.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::wallet_backup::{CARD_ARTWORK_ASSETS, valid_asset};

const INTENT_VERSION: u32 = 2;
const PDF_LEAF: &str = "cardBackgroundCombined.pdf";
const MAX_ASSET_BYTES: usize = 32 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Intent {
    pub operation_dir: PathBuf,
    pub udid: String,
    pub card_hash: String,
    pub assets: BTreeMap<String, Vec<u8>>,
    pub absent_assets: Vec<String>,
    pub phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IntentState {
    version: u32,
    revision: u32,
    operation_id: String,
    sequence: u64,
    created_unix_ms: u128,
    udid: String,
    card_hash: String,
    phase: String,
    assets: BTreeMap<String, AssetRecord>,
    #[serde(default)]
    absent_assets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AssetRecord {
    size: usize,
    sha256: String,
}

fn intent_root() -> PathBuf {
    crate::storage::root().join("bank-change-intents")
}

#[derive(Debug)]
pub(crate) struct PendingCloudHistory;
impl std::fmt::Display for PendingCloudHistory {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {f.write_str("uncertain intent needs local recovery before cloud export")}
}
impl std::error::Error for PendingCloudHistory {}
pub(crate) fn cloud_operation_dirs(root:&Path,udid:&str,hash:&str)->Result<Vec<PathBuf>> {
    let states=matching_states_at(root,udid,hash,false)?;
    anyhow::ensure!(states.iter().all(|(_,state)| matches!(state.phase.as_str(),"applied"|"restored")),PendingCloudHistory);
    Ok(states.into_iter().map(|(path,_)|path).collect())
}

/// Narrow recovery inventory: only interrupted writes, never an intentionally
/// applied design. Existing identity, history and payload checks stay mandatory.
pub(crate) fn interrupted_restorations_for_device(udid:&str)->Result<Vec<Intent>> {
    let root=intent_root();
    interrupted_restorations_at(&root,udid)
}
fn interrupted_restorations_at(root:&Path,udid:&str)->Result<Vec<Intent>> {
    let mut hashes=std::collections::BTreeSet::new();
    for path in state_paths(&root,false)? {
        let state=latest_metadata(&path)?;
        if state.udid==udid {hashes.insert(state.card_hash);}
    }
    let mut result=Vec::new();
    for hash in hashes {
        if let Some(intent)=load_latest_at(&root,udid,&hash)? {
            if matches!(intent.phase.as_str(),"prepared"|"originals_verified") {result.push(intent);}
        }
    }
    Ok(result)
}

pub(crate) fn validate_cloud_chain(root:&Path,udid:&str,hash:&str)->Result<()> {
    for path in state_paths(root,false)? { let state=latest_state(&path)?;
        anyhow::ensure!(state.udid==udid && state.card_hash==hash,"cloud intent identity mixed");
        for entry in fs::read_dir(&path)? { let file=entry?.path(); let name=file.file_name().unwrap().to_string_lossy();
            if name.starts_with("state-") && name.ends_with(".json") {
                let meta=fs::symlink_metadata(&file)?; anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=MAX_STATE_BYTES,"unsafe historical intent");
                let historical:IntentState=serde_json::from_slice(&fs::read(&file)?)?; validate_state(&path,&historical)?;
                anyhow::ensure!(historical.udid==udid && historical.card_hash==hash && historical.revision<=state.revision,"historical intent identity mismatch");
                anyhow::ensure!(name==format!("state-{:04}-{}.json",historical.revision,historical.phase),"historical intent filename mismatch");
            }
        }
    }
    cloud_operation_dirs(root,udid,hash)?; Ok(())
}

/// Read a complete isolated remote union, never the live mixed local history.
/// A restored final operation grants no authority; ambiguous chains fail closed.
pub(crate) fn active_cloud_intent_at(root:&Path,udid:&str,hash:&str)->Result<Option<Intent>> {
    validate_cloud_chain(root,udid,hash)?;
    for path in state_paths(root,false)? {
        let operation=path.file_name().and_then(|n|n.to_str()).context("invalid proof operation")?;
        recovery_evidence_at(root,operation,udid,hash)?;
    }
    let intent=load_latest_at(root,udid,hash)?;
    Ok(intent.filter(|intent|intent.phase=="applied"))
}

pub(crate) fn validate_persistence_store_at(root:&Path)->Result<()> {
    let mut identities=std::collections::BTreeSet::new();
    for directory in state_paths(root,false)? { let state=latest_state(&directory)?; identities.insert((state.udid,state.card_hash)); }
    for (udid,hash) in identities {
        // Validate every historical revision, including unfinished local intents;
        // only cloud export additionally requires completed/applicable phases.
        for (directory,latest) in matching_states_at(root,&udid,&hash,false)? {
            for entry in fs::read_dir(&directory)? { let file=entry?.path(); let name=file.file_name().unwrap().to_string_lossy();
                if name.starts_with("state-") && name.ends_with(".json") {
                    let meta=fs::symlink_metadata(&file)?;
                    anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=MAX_STATE_BYTES,"historical state unsafe");
                    let state:IntentState=serde_json::from_slice(&fs::read(&file)?)?; validate_state(&directory,&state)?;
                    anyhow::ensure!(state.udid==udid && state.card_hash==hash && state.revision<=latest.revision &&
                        name==format!("state-{:04}-{}.json",state.revision,state.phase),"historical state identity mismatch");
                }
            }
        }
    } Ok(())
}
pub(crate) fn review_persistence_store_at(root:&Path,vault:&Path)->Result<()> {
    let mut identities=std::collections::BTreeSet::new();
    for directory in state_paths(root,false)? { identities.insert(operation_identity(&directory)?); }
    for (udid,hash) in identities {
        let result=(|| {for (directory,latest) in matching_states_at(root,&udid,&hash,false)? {
            for entry in fs::read_dir(&directory)? {let file=entry?.path();let name=file.file_name().unwrap().to_string_lossy();
                if name.starts_with("state-") && name.ends_with(".json") {
                    crate::storage::safe_chain(&file)?;let meta=fs::symlink_metadata(&file)?;anyhow::ensure!(meta.is_file() && meta.len()<=MAX_STATE_BYTES,"historical state unsafe");
                    let state:IntentState=serde_json::from_slice(&fs::read(&file)?)?;validate_state(&directory,&state)?;
                    anyhow::ensure!(state.udid==udid && state.card_hash==hash && state.revision<=latest.revision && name==format!("state-{:04}-{}.json",state.revision,state.phase),"historical identity invalid");
                }
            }
        }Ok::<(),anyhow::Error>(())})();
        if result.is_err() {crate::storage::block_card_at(vault,&udid,&hash)?;}
    }Ok(())
}
pub(crate) fn operation_identity(directory:&Path)->Result<(String,String)> {
    let state=latest_metadata(directory)?;Ok((state.udid,state.card_hash))
}
#[cfg(test)] pub(crate) fn save_fixture_prepared_at(root:&Path,udid:&str,hash:&str,pairs:&[(String,Vec<u8>)])->Result<PathBuf> {
    save_intent_at(root,udid,hash,pairs,true)
}
#[cfg(test)] pub(crate) fn apply_fixture_at(directory:&Path)->Result<()> {
    append_phase(directory,"applied")
}
#[cfg(test)] pub(crate) fn save_fixture_applied_at(root:&Path,udid:&str,hash:&str,pairs:&[(String,Vec<u8>)])->Result<PathBuf> {
    let directory=save_intent_at(root,udid,hash,pairs,true)?;append_phase(&directory,"applied")?;Ok(directory)
}
#[cfg(test)] pub(crate) fn restore_fixture_at(directory:&Path)->Result<()> {
    append_phase(directory,"originals_verified")?;append_phase(directory,"restored")
}

fn identity_key(udid: &str, card_hash: &str) -> Result<String> {
    anyhow::ensure!(!udid.is_empty() && udid.len() <= 256 && udid.trim() == udid,
        "Invalid device identity for bank write intent");
    anyhow::ensure!(crate::wallet_export::is_exact_card_hash(card_hash),
        "Invalid literal card hash for bank write intent");
    let mut bytes = Vec::with_capacity(udid.len() + card_hash.len() + 1);
    bytes.extend_from_slice(udid.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(card_hash.as_bytes());
    sha256(&bytes)
}

fn valid_operation_id(value: &str) -> bool {
    value.len() == 20 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)
        .with_context(|| format!("Refusing to replace intent evidence at {}", path.display()))?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    anyhow::ensure!(fs::read(path)? == bytes, "Write-intent evidence failed readback verification");
    Ok(())
}

struct CreationLock { path: PathBuf }
impl Drop for CreationLock {
    fn drop(&mut self) { let _ = fs::remove_file(&self.path); }
}

fn acquire_creation_lock(root: &Path) -> Result<CreationLock> {
    fs::create_dir_all(root)?;
    let root_meta = fs::symlink_metadata(root)?;
    anyhow::ensure!(root_meta.is_dir() && !root_meta.file_type().is_symlink(),
        "Bank write-intent root must be a real directory");
    let path = root.join(".create.lock");
    let mut file = OpenOptions::new().write(true).create_new(true).open(&path)
        .context("Another bank write intent is being created, or an interrupted creation needs review")?;
    file.write_all(std::process::id().to_string().as_bytes())?;
    file.sync_all()?;
    Ok(CreationLock { path })
}

fn validate_slot_set<'a>(present: impl Iterator<Item = &'a str>, absent: &[String]) -> Result<()> {
    let present: Vec<_> = present.collect();
    let static_set = absent.is_empty() && present.len() == 3
        && CARD_ARTWORK_ASSETS.iter().all(|leaf| present.contains(leaf));
    let animated_set = absent == [PDF_LEAF] && present.len() == 2
        && CARD_ARTWORK_ASSETS.iter().filter(|leaf| **leaf != PDF_LEAF).all(|leaf| present.contains(leaf));
    anyhow::ensure!(static_set || animated_set,
        "A bank intent requires all three assets or both PNGs with explicit PDF absence");
    Ok(())
}

fn validate_assets(pairs: &[(String, Vec<u8>)]) -> Result<BTreeMap<String, AssetRecord>> {
    let absent = if pairs.len() == 2 { vec![PDF_LEAF.to_string()] } else { Vec::new() };
    validate_slot_set(pairs.iter().map(|(leaf, _)| leaf.as_str()), &absent)?;
    let mut records = BTreeMap::new();
    for (leaf, bytes) in pairs {
        anyhow::ensure!(CARD_ARTWORK_ASSETS.contains(&leaf.as_str()),
            "Unexpected bank artwork override leaf: {leaf}");
        anyhow::ensure!(bytes.len() <= MAX_ASSET_BYTES && valid_asset(leaf, bytes),
            "Bank artwork override is invalid: {leaf}");
        anyhow::ensure!(records.insert(leaf.clone(), AssetRecord {
            size: bytes.len(), sha256: sha256(bytes)?,
        }).is_none(), "Duplicate bank artwork override leaf: {leaf}");
    }
    Ok(records)
}

fn latest_metadata(operation_dir: &Path) -> Result<IntentState> {
    crate::storage::safe_chain(operation_dir)?;
    let mut paths = fs::read_dir(operation_dir)?.collect::<std::io::Result<Vec<_>>>()?.into_iter()
        .map(|item| item.path())
        .filter(|path| path.file_name().and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("state-") && n.ends_with(".json")))
        .collect::<Vec<_>>();
    paths.sort();
    let path = paths.pop().context("Write-intent operation has no committed state")?;
    let meta = fs::symlink_metadata(&path)?;
    anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len() <= MAX_STATE_BYTES,
        "Write-intent state file is unsafe or too large");
    let state: IntentState = serde_json::from_slice(&fs::read(path)?)
        .context("Write-intent state is corrupt")?;
    identity_key(&state.udid,&state.card_hash)?;
    anyhow::ensure!(valid_operation_id(&state.operation_id) && operation_dir.file_name().and_then(|n|n.to_str())==Some(&state.operation_id),"intent metadata identity invalid");
    Ok(state)
}
fn latest_state(operation_dir:&Path)->Result<IntentState> {let state=latest_metadata(operation_dir)?;validate_state(operation_dir,&state)?;Ok(state)}

fn validate_state(operation_dir: &Path, state: &IntentState) -> Result<()> {
    anyhow::ensure!(matches!(state.version, 1 | INTENT_VERSION) && valid_operation_id(&state.operation_id),
        "Unsupported or invalid bank write-intent identity");
    anyhow::ensure!(state.revision > 0 && state.sequence > 0,
        "Write-intent revision or sequence is invalid");
    anyhow::ensure!(operation_dir.file_name().and_then(|n| n.to_str()) == Some(state.operation_id.as_str()),
        "Write-intent directory does not match its operation ID");
    anyhow::ensure!(matches!(state.phase.as_str(), "prepared" | "applied" | "originals_verified" | "restored"),
        "Unknown bank write-intent phase");
    let expected_identity = identity_key(&state.udid, &state.card_hash)?;
    anyhow::ensure!(!expected_identity.is_empty(), "Invalid intent identity binding");
    anyhow::ensure!(state.version != 1 || state.absent_assets.is_empty(), "Version one intent cannot declare absence");
    validate_slot_set(state.assets.keys().map(String::as_str), &state.absent_assets)?;
    for (leaf, record) in &state.assets {
        anyhow::ensure!(record.size > 0 && record.size <= MAX_ASSET_BYTES
            && record.sha256.len() == 64 && record.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
            "Write-intent asset metadata is invalid: {leaf}");
        let bytes = read_payload(operation_dir, leaf, record.size)?;
        anyhow::ensure!(sha256(&bytes)? == record.sha256 && valid_asset(leaf, &bytes),
            "Write-intent payload failed integrity or format validation: {leaf}");
    }
    Ok(())
}

fn read_payload(operation_dir: &Path, leaf: &str, expected_size: usize) -> Result<Vec<u8>> {
    anyhow::ensure!(CARD_ARTWORK_ASSETS.contains(&leaf), "Unexpected write-intent payload leaf");
    let path = operation_dir.join(leaf);
    let metadata = fs::symlink_metadata(&path).with_context(|| format!("Missing intent payload {leaf}"))?;
    anyhow::ensure!(metadata.is_file() && !metadata.file_type().is_symlink()
        && metadata.len() <= MAX_ASSET_BYTES as u64 && metadata.len() == expected_size as u64,
        "Intent payload is unsafe or has the wrong size: {leaf}");
    Ok(fs::read(path)?)
}

fn state_paths(root: &Path, own_creation_lock: bool) -> Result<Vec<PathBuf>> {
    crate::storage::safe_chain(root)?;
    if !root.exists() { return Ok(Vec::new()); }
    let metadata = fs::symlink_metadata(root)?;
    anyhow::ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Bank write-intent root must be a real directory");
    let mut dirs = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        crate::storage::safe_chain(&path)?;
        if path.file_name().and_then(|n| n.to_str()) == Some(".create.lock") {
            if own_creation_lock { continue; }
            bail!("An interrupted or concurrent bank write-intent creation needs review at {}", path.display());
        }
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() && !meta.file_type().is_symlink() {
            let has_any_files = fs::read_dir(&path)?.next().is_some();
            if has_any_files { dirs.push(path); }
        }
    }
    Ok(dirs)
}

fn load_latest_at(root: &Path, udid: &str, card_hash: &str) -> Result<Option<Intent>> {
    load_latest_at_with_lock(root, udid, card_hash, false)
}

fn matching_states_at(root: &Path, udid: &str, card_hash: &str,
    own_creation_lock: bool) -> Result<Vec<(PathBuf, IntentState)>> {
    let _ = identity_key(udid, card_hash)?;
    let mut found = Vec::new();
    for directory in state_paths(root, own_creation_lock)? {
        let state = latest_metadata(&directory)?;
        // Unknown identities/JSON remain global failures; known foreign payloads
        // are checked only when that exact device/card is requested.
        for entry in fs::read_dir(&directory)? {let file=entry?.path();let name=file.file_name().unwrap().to_string_lossy();
            if name.starts_with("state-") && name.ends_with(".json") {
                crate::storage::safe_chain(&file)?;let metadata=fs::symlink_metadata(&file)?;
                anyhow::ensure!(metadata.is_file() && metadata.len()<=MAX_STATE_BYTES,"historical state unsafe");
                let historical:IntentState=serde_json::from_slice(&fs::read(&file)?)?;identity_key(&historical.udid,&historical.card_hash)?;
                anyhow::ensure!(historical.udid==state.udid && historical.card_hash==state.card_hash && historical.operation_id==state.operation_id,"historical identity unknown");
                if state.udid==udid && state.card_hash==card_hash {validate_state(&directory,&historical)?;}
            }
        }
        if state.udid == udid && state.card_hash == card_hash {
            validate_state(&directory,&state)?;
            found.push((directory, state));
        }
    }
    found.sort_by_key(|(_, state)| (state.sequence, state.created_unix_ms));
    for pair in found.windows(2) {
        let older = &pair[0].1;
        let newer = &pair[1].1;
        // Historical clients restarted sequence at one. Accept only a strictly
        // dated chain whose preceding operation is already durably restored.
        // Preserve every immutable state and payload; ambiguous evidence blocks.
        anyhow::ensure!(older.sequence < newer.sequence
            || (older.sequence == 1 && newer.sequence == 1
                && older.phase == "restored" && older.created_unix_ms > 0
                && older.created_unix_ms < newer.created_unix_ms),
            "Ambiguous duplicate bank write-intent sequence for this device/card");
    }
    let unresolved = found.iter().filter(|(_, state)| state.phase != "restored").count();
    anyhow::ensure!(unresolved <= 1,
        "Multiple unresolved bank write intents exist for this device/card; manual review is required");
    anyhow::ensure!(found.iter().take(found.len().saturating_sub(1))
        .all(|(_, state)| state.phase == "restored"),
        "An older unresolved bank write intent exists; manual review is required");
    Ok(found)
}

fn load_latest_at_with_lock(root: &Path, udid: &str, card_hash: &str,
    own_creation_lock: bool) -> Result<Option<Intent>> {
    let mut found = matching_states_at(root, udid, card_hash, own_creation_lock)?;
    let Some((operation_dir, state)) = found.pop() else { return Ok(None) };
    let assets = state.assets.iter().map(|(leaf, record)| {
        Ok((leaf.to_string(), read_payload(&operation_dir, leaf, record.size)?))
    }).collect::<Result<BTreeMap<_, _>>>()?;
    Ok(Some(Intent { operation_dir, udid: state.udid, card_hash: state.card_hash,
        assets, absent_assets: state.absent_assets, phase: state.phase }))
}

// Recovery uses exact recorded operation IDs even while a crash-safe backfill
// temporarily leaves two unresolved histories. Never select an arbitrary latest.
pub(crate) fn recovery_evidence_at(root:&Path,operation:&str,udid:&str,hash:&str)->Result<BTreeMap<String,Vec<u8>>> {
    anyhow::ensure!(valid_operation_id(operation),"invalid recovery operation ID");
    let directory=root.join(operation);crate::storage::safe_chain(&directory)?;
    let latest=latest_state(&directory)?;
    anyhow::ensure!(latest.udid==udid && latest.card_hash==hash,"recovery operation identity mismatch");
    let mut files=BTreeMap::new();let mut states=Vec::new();
    for entry in fs::read_dir(&directory)? {
        let path=entry?.path();crate::storage::safe_chain(&path)?;
        let name=path.file_name().and_then(|n|n.to_str()).context("invalid operation evidence name")?.to_string();
        let meta=fs::symlink_metadata(&path)?;
        anyhow::ensure!(meta.is_file() && !meta.file_type().is_symlink() && meta.len()<=MAX_ASSET_BYTES as u64,"unsafe operation recovery evidence");
        let bytes=fs::read(&path)?;
        if name.starts_with("state-") && name.ends_with(".json") {
            anyhow::ensure!(bytes.len()<=MAX_STATE_BYTES as usize,"oversized recovery revision");
            let state:IntentState=serde_json::from_slice(&bytes)?;validate_state(&directory,&state)?;
            anyhow::ensure!(state.udid==udid && state.card_hash==hash && name==format!("state-{:04}-{}.json",state.revision,state.phase),"historical recovery identity mismatch");
            states.push(state);
        }else {anyhow::ensure!(latest.assets.contains_key(&name),"undeclared recovery operation file");}
        files.insert(name,bytes);
    }
    states.sort_by_key(|state|state.revision);
    anyhow::ensure!(states.first().is_some_and(|s|s.revision==1 && s.phase=="prepared"),"recovery history has no prepared start");
    for pair in states.windows(2) {
        let (old,new)=(&pair[0],&pair[1]);
        let transition=matches!((old.phase.as_str(),new.phase.as_str()),("prepared","applied")|("prepared","originals_verified")|("applied","originals_verified")|("originals_verified","restored"));
        let mut comparable=old.clone();comparable.revision=new.revision;comparable.phase=new.phase.clone();
        anyhow::ensure!(new.revision==old.revision+1 && transition && serde_json::to_vec(&comparable)?==serde_json::to_vec(new)?,"recovery operation revisions are not an immutable valid chain");
    }
    Ok(files)
}
pub(crate) fn validate_recovery_backfill_at(root:&Path,operation:&str,udid:&str,hash:&str,incoming:&BTreeMap<String,Vec<u8>>)->Result<()> {
    let directory=root.join(operation);
    if !directory.exists() {return Ok(());}
    let current=recovery_evidence_at(root,operation,udid,hash)?;
    for (name,bytes) in incoming {
        if let Some(existing)=current.get(name) {anyhow::ensure!(existing==bytes,"immutable recovery backfill conflict");continue;}
        anyhow::ensure!(name.starts_with("state-"),"recovery backfill cannot replace missing managed assets");
        let next:IntentState=serde_json::from_slice(bytes)?;
        for (existing,bytes) in &current {if existing.starts_with("state-") {
            let old:IntentState=serde_json::from_slice(bytes)?;
            anyhow::ensure!(old.revision!=next.revision,"different recovery phases share one revision");
        }}
    }Ok(())
}
pub(crate) fn complete_exact_recovery_operation(operation:&str,udid:&str,hash:&str,physical:&crate::flasher::VerifiedOriginalRestoration,plan:&crate::bank_recovery::RecoveryPlan)->Result<()> {
    anyhow::ensure!(physical.matches(plan),"physical original proof is required to complete recovery");
    let root=plan.root().join("bank-change-intents");recovery_evidence_at(&root,operation,udid,hash)?;
    let path=root.join(operation);let phase=latest_state(&path)?.phase;
    if matches!(phase.as_str(),"prepared"|"applied") {append_phase(&path,"originals_verified")?;}
    if latest_state(&path)?.phase=="originals_verified" {append_phase(&path,"restored")?;}
    anyhow::ensure!(latest_state(&path)?.phase=="restored","recovery phase completion failed");Ok(())
}

fn append_phase(operation_dir: &Path, requested_phase: &str) -> Result<()> {
    let mut state = latest_state(operation_dir)?;
    let allowed = match (state.phase.as_str(), requested_phase) {
        ("prepared", "applied")
        | ("prepared", "originals_verified")
        | ("applied", "originals_verified")
        | ("originals_verified", "restored") => true,
        _ => false,
    };
    anyhow::ensure!(allowed, "Invalid bank write-intent phase transition {} -> {requested_phase}", state.phase);
    state.revision = state.revision.checked_add(1).context("Write-intent revision overflow")?;
    state.phase = requested_phase.to_string();
    let path = operation_dir.join(format!("state-{:04}-{}.json", state.revision, state.phase));
    write_new(&path, &serde_json::to_vec_pretty(&state)?)
}

fn validate_managed_operation_dir(operation_dir: &Path) -> Result<()> {
    anyhow::ensure!(operation_dir.parent() == Some(intent_root().as_path()),
        "Write-intent operation is outside the managed intent root");
    let metadata = fs::symlink_metadata(operation_dir)?;
    anyhow::ensure!(metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Write-intent operation directory must be a real directory");
    Ok(())
}

fn save_intent_at(root: &Path, udid: &str, card_hash: &str,
    pairs: &[(String, Vec<u8>)], baseline_verified: bool) -> Result<PathBuf> {
    anyhow::ensure!(baseline_verified, "A verified bank baseline is required before recording a write intent");
    let records = validate_assets(pairs)?;
    let _lock = acquire_creation_lock(root)?;
    let previous = matching_states_at(root, udid, card_hash, true)?;
    if let Some((_, pending)) = previous.iter().find(|(_, state)| state.phase != "restored") {
        bail!("A previous bank write intent is still {}; resolve it before another apply", pending.phase);
    }
    let sequence = previous.iter().map(|(_, state)| state.sequence).max().unwrap_or(0)
        .checked_add(1).context("Bank write-intent sequence overflow")?;
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let operation_id = crate::flasher::generate_token();
    anyhow::ensure!(valid_operation_id(&operation_id), "Generated write-intent ID is invalid");
    let directory = root.join(&operation_id);
    fs::create_dir(&directory)?;
    for (leaf, bytes) in pairs { write_new(&directory.join(leaf), bytes)?; }
    // Ordinary three-file writes retain the version-one contract. Only the
    // explicit two-PNG/absent-PDF experiment needs the new schema version.
    let state = IntentState { version: if pairs.len() == 2 { INTENT_VERSION } else { 1 }, revision: 1, operation_id, sequence,
        created_unix_ms: timestamp, udid: udid.to_string(), card_hash: card_hash.to_string(),
        phase: "prepared".into(), assets: records,
        absent_assets: if pairs.len() == 2 { vec![PDF_LEAF.to_string()] } else { Vec::new() } };
    write_new(&directory.join("state-0001-prepared.json"), &serde_json::to_vec_pretty(&state)?)?;
    Ok(directory)
}

/// Record payloads before native apply. Requires a fully verified bank baseline
/// for the exact device/card and blocks while a prior intent is unresolved.
pub fn save_intent(udid: &str, card_hash: &str,
    pairs: &[(String, Vec<u8>)]) -> Result<PathBuf> {
    let baseline = crate::bank_backup::load_baseline(udid, card_hash)?;
    anyhow::ensure!(baseline.is_some(), "A verified bank baseline is required before applying artwork");
    save_intent_at(&intent_root(), udid, card_hash, pairs, true)
}

/// Load the newest durable intent for the exact device/card identity.
pub fn load_latest_intent(udid: &str, card_hash: &str) -> Result<Option<Intent>> {
    load_latest_at(&intent_root(), udid, card_hash)
}

pub fn has_unrestored_intent(udid: &str, card_hash: &str) -> bool {
    load_latest_intent(udid, card_hash).ok().flatten().is_some_and(|intent| intent.phase != "restored")
}

pub fn mark_applied(operation_dir: &Path) -> Result<()> {
    validate_managed_operation_dir(operation_dir)?;
    append_phase(operation_dir, "applied")
}

pub fn mark_restored(operation_dir: &Path) -> Result<()> {
    validate_managed_operation_dir(operation_dir)?;
    append_phase(operation_dir, "restored")
}

/// Checkpoint that every original-present asset has exact protected readback
/// and every baseline-absent override has been proven absent. A restore retry
/// can then resume at cache invalidation without repeating uncertain USB writes.
pub fn mark_originals_verified(operation_dir: &Path) -> Result<()> {
    validate_managed_operation_dir(operation_dir)?;
    append_phase(operation_dir, "originals_verified")
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
    let name: Vec<u16> = "SHA256\0".encode_utf16().collect();
    let mut algorithm = std::ptr::null_mut();
    let mut digest = [0u8; 32];
    unsafe {
        anyhow::ensure!(BCryptOpenAlgorithmProvider(&mut algorithm, name.as_ptr(), std::ptr::null(), 0) == 0,
            "Could not initialize intent SHA-256");
        let status = BCryptHash(algorithm, std::ptr::null(), 0, bytes.as_ptr(),
            u32::try_from(bytes.len())?, digest.as_mut_ptr(), digest.len() as u32);
        BCryptCloseAlgorithmProvider(algorithm, 0);
        anyhow::ensure!(status == 0, "Could not compute intent SHA-256");
    }
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_recovery_inventory_excludes_applied_restored_and_foreign_cards() {
        let root=std::env::temp_dir().join(format!("bank-interrupted-{}",crate::flasher::generate_token()));
        let hash="OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        let operation=save_intent_at(&root,"device-A",hash,&pairs(),true).unwrap();
        assert!(interrupted_restorations_at(&root,"device-B").unwrap().is_empty());
        assert_eq!(interrupted_restorations_at(&root,"device-A").unwrap().len(),1);
        append_phase(&operation,"applied").unwrap();
        assert!(interrupted_restorations_at(&root,"device-A").unwrap().is_empty());
        append_phase(&operation,"originals_verified").unwrap();
        assert_eq!(interrupted_restorations_at(&root,"device-A").unwrap().len(),1);
        append_phase(&operation,"restored").unwrap();
        assert!(interrupted_restorations_at(&root,"device-A").unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn two_png_intent_records_explicit_pdf_absence_and_restores_without_missing_payload_reads() {
        let root = std::env::temp_dir().join(format!("bank-intent-apng-{}-{}", std::process::id(), crate::flasher::generate_token()));
        let hash = "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        let png_pairs = pairs().into_iter().filter(|(leaf,_)| leaf != PDF_LEAF).collect::<Vec<_>>();
        let directory = save_intent_at(&root, "device-A", hash, &png_pairs, true).unwrap();
        let intent = load_latest_at(&root,"device-A",hash).unwrap().unwrap();
        assert_eq!(intent.assets.len(), 2);
        assert_eq!(intent.absent_assets, [PDF_LEAF]);
        assert!(!directory.join(PDF_LEAF).exists());
        append_phase(&directory,"applied").unwrap();
        append_phase(&directory,"originals_verified").unwrap();
        append_phase(&directory,"restored").unwrap();
        assert_eq!(load_latest_at(&root,"device-A",hash).unwrap().unwrap().phase,"restored");
        let v2_evidence = fs::read(directory.join("state-0004-restored.json")).unwrap();
        let next = save_intent_at(&root,"device-A",hash,&pairs(),true).unwrap();
        let next_state = latest_state(&next).unwrap();
        assert_eq!(next_state.version,1);
        assert_eq!(next_state.sequence,2);
        let latest = load_latest_at(&root,"device-A",hash).unwrap().unwrap();
        assert_eq!(latest.operation_dir,next);
        assert_eq!(latest.assets.len(),3);
        assert!(latest.absent_assets.is_empty());
        assert_eq!(fs::read(directory.join("state-0004-restored.json")).unwrap(),v2_evidence);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn v1_three_asset_records_load_without_absence_field_and_bad_v2_slots_fail_closed() {
        let root = std::env::temp_dir().join(format!("bank-intent-v1-{}-{}", std::process::id(), crate::flasher::generate_token()));
        let hash = "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        let directory = save_intent_at(&root,"device-A",hash,&pairs(),true).unwrap();
        let path = directory.join("state-0001-prepared.json");
        let mut json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        json["version"] = serde_json::json!(1);
        json.as_object_mut().unwrap().remove("absent_assets");
        fs::write(&path,serde_json::to_vec(&json).unwrap()).unwrap();
        let intent = load_latest_at(&root,"device-A",hash).unwrap().unwrap();
        assert_eq!(intent.assets.len(),3); assert!(intent.absent_assets.is_empty());
        let mut state = latest_state(&directory).unwrap(); state.version = 2;
        state.assets.remove(PDF_LEAF);
        assert!(validate_state(&directory,&state).is_err());
        state.absent_assets = vec![PDF_LEAF.into(),PDF_LEAF.into()];
        assert!(validate_state(&directory,&state).is_err());
        state.absent_assets = vec!["cardBackgroundCombined@2x.png".into()];
        assert!(validate_state(&directory,&state).is_err());
        state.absent_assets = vec![PDF_LEAF.into()];
        assert!(validate_state(&directory,&state).is_ok());
        state.version = 1;
        assert!(validate_state(&directory,&state).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(4, 4).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        bytes
    }

    #[test] fn remote_recovery_proof_rejects_revision_aliases_and_changed_operation_metadata() {
        let hash="OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        for alias in [false,true] {
            let root=std::env::temp_dir().join(format!("ws-proof-history-{}",crate::flasher::generate_token()));
            let operation=save_fixture_applied_at(&root,"synthetic-proof-device",hash,&pairs()).unwrap();
            assert!(active_cloud_intent_at(&root,"synthetic-proof-device",hash).unwrap().is_some());
            if alias {
                restore_fixture_at(&operation).unwrap();
                let old=operation.join("state-0004-restored.json");let mut state:IntentState=serde_json::from_slice(&fs::read(&old).unwrap()).unwrap();state.revision=3;
                fs::remove_file(old).unwrap();fs::write(operation.join("state-0003-restored.json"),serde_json::to_vec(&state).unwrap()).unwrap();
            }else {
                let path=operation.join("state-0002-applied.json");let mut state:IntentState=serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();state.sequence+=1;
                fs::write(path,serde_json::to_vec(&state).unwrap()).unwrap();
            }
            assert!(active_cloud_intent_at(&root,"synthetic-proof-device",hash).is_err());fs::remove_dir_all(root).unwrap();
        }
    }
    fn pairs() -> Vec<(String, Vec<u8>)> {
        CARD_ARTWORK_ASSETS.iter().map(|leaf| (leaf.to_string(), if leaf.ends_with(".pdf") {
            b"%PDF-1.7\n%%EOF".to_vec()
        } else { png() })).collect()
    }
    #[test] fn customer_foreign_payload_corruption_is_scoped_but_unknown_json_blocks() {
        let root=std::env::temp_dir().join(format!("ws-history-fixture-{}",crate::flasher::generate_token()));
        let hash="OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        let a=save_intent_at(&root,"synthetic-A",hash,&pairs(),true).unwrap();
        let b=save_intent_at(&root,"synthetic-B",hash,&pairs(),true).unwrap();
        fs::write(a.join("cardBackgroundCombined@3x.png"),b"corrupted fixture").unwrap();
        assert!(load_latest_at(&root,"synthetic-A",hash).is_err());assert!(load_latest_at(&root,"synthetic-B",hash).unwrap().is_some());
        let vault=std::env::temp_dir().join(format!("ws-review-fixture-{}",crate::flasher::generate_token())); review_persistence_store_at(&root,&vault).unwrap();
        assert!(crate::storage::ensure_card_ready_at(&vault,"synthetic-A",hash).is_err());assert!(crate::storage::ensure_card_ready_at(&vault,"synthetic-B",hash).is_ok());
        assert!(b.is_dir());fs::write(a.join("state-0001-prepared.json"),b"not JSON").unwrap();
        assert!(load_latest_at(&root,"synthetic-B",hash).is_err());fs::remove_dir_all(root).unwrap();fs::remove_dir_all(vault).unwrap();
    }

    #[test]
    fn legacy_sequence_one_history_selects_the_newer_active_intent_and_next_sequence() {
        let root = std::env::temp_dir().join(format!("bank-intent-legacy-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let hash = "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        let first = save_intent_at(&root, "device-A", hash, &pairs(), true).unwrap();
        append_phase(&first, "applied").unwrap();
        append_phase(&first, "originals_verified").unwrap();
        append_phase(&first, "restored").unwrap();
        let old_bytes = fs::read(first.join("state-0004-restored.json")).unwrap();
        let mut newer = latest_state(&first).unwrap();
        newer.operation_id = crate::flasher::generate_token();
        newer.created_unix_ms += 1;
        newer.revision = 2;
        newer.phase = "applied".into();
        let second = root.join(&newer.operation_id);
        fs::create_dir(&second).unwrap();
        for leaf in CARD_ARTWORK_ASSETS { fs::copy(first.join(leaf), second.join(leaf)).unwrap(); }
        write_new(&second.join("state-0002-applied.json"), &serde_json::to_vec(&newer).unwrap()).unwrap();
        let active = load_latest_at(&root, "device-A", hash).unwrap().unwrap();
        assert_eq!(active.operation_dir, second);
        assert_eq!(active.phase, "applied");
        assert!(save_intent_at(&root, "device-A", hash, &pairs(), true).is_err());
        append_phase(&second, "originals_verified").unwrap();
        append_phase(&second, "restored").unwrap();
        let next = save_intent_at(&root, "device-A", hash, &pairs(), true).unwrap();
        assert_eq!(latest_state(&next).unwrap().sequence, 2);
        assert_eq!(fs::read(first.join("state-0004-restored.json")).unwrap(), old_bytes);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn legacy_duplicate_dates_and_multiple_active_intents_remain_blocked() {
        for older_restored in [false, true] {
            let root = std::env::temp_dir().join(format!("bank-intent-ambiguous-{}-{}",
                std::process::id(), crate::flasher::generate_token()));
            let hash = "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
            let first = save_intent_at(&root, "device-A", hash, &pairs(), true).unwrap();
            if older_restored {
                append_phase(&first, "originals_verified").unwrap();
                append_phase(&first, "restored").unwrap();
            }
            let mut newer = latest_state(&first).unwrap();
            newer.operation_id = crate::flasher::generate_token();
            newer.revision = 1;
            newer.phase = "prepared".into();
            if !older_restored { newer.created_unix_ms += 1; }
            let second = root.join(&newer.operation_id);
            fs::create_dir(&second).unwrap();
            for leaf in CARD_ARTWORK_ASSETS { fs::copy(first.join(leaf), second.join(leaf)).unwrap(); }
            write_new(&second.join("state-0001-prepared.json"), &serde_json::to_vec(&newer).unwrap()).unwrap();
            assert!(load_latest_at(&root, "device-A", hash).is_err());
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn intent_payloads_are_immutable_and_only_one_unrestored_intent_is_allowed() {
        let root = std::env::temp_dir().join(format!("bank-intent-test-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let first = save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &pairs(), true).unwrap();
        let loaded = load_latest_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").unwrap().unwrap();
        assert_eq!(loaded.phase, "prepared");
        assert_eq!(loaded.assets.len(), 3);
        assert!(save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &pairs(), true).is_err());
        append_phase(&first, "applied").unwrap();
        assert_eq!(load_latest_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").unwrap().unwrap().phase, "applied");
        assert!(save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &pairs(), true).is_err());
        append_phase(&first, "originals_verified").unwrap();
        assert_eq!(load_latest_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").unwrap().unwrap().phase, "originals_verified");
        assert!(append_phase(&first, "applied").is_err());
        append_phase(&first, "restored").unwrap();
        let second = save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &pairs(), true).unwrap();
        let newest = load_latest_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").unwrap().unwrap();
        assert_eq!(newest.operation_dir, second);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepared_intent_can_checkpoint_verified_original_state() {
        let root = std::env::temp_dir().join(format!("bank-intent-restore-checkpoint-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let operation = save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &pairs(), true).unwrap();
        append_phase(&operation, "originals_verified").unwrap();
        let loaded = load_latest_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").unwrap().unwrap();
        assert_eq!(loaded.phase, "originals_verified");
        append_phase(&operation, "restored").unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn tampering_and_cross_device_identity_fail_closed() {
        let root = std::env::temp_dir().join(format!("bank-intent-tamper-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let path = save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &pairs(), true).unwrap();
        assert!(load_latest_at(&root, "device-B", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").unwrap().is_none());
        fs::write(path.join("cardBackgroundCombined@3x.png"), b"tampered").unwrap();
        assert!(load_latest_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn baseline_gate_and_exact_asset_set_are_required() {
        let root = std::env::temp_dir().join(format!("bank-intent-gate-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        assert!(save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &pairs(), false).is_err());
        let mut invalid = pairs();
        invalid.remove(0); // A single PNG plus PDF is still incomplete and forbidden.
        assert!(save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &invalid, true).is_err());
        assert!(!root.exists());
    }

    #[test]
    fn multiple_unresolved_intents_and_interrupted_lock_are_explicit_errors() {
        let root = std::env::temp_dir().join(format!("bank-intent-conflict-{}-{}",
            std::process::id(), crate::flasher::generate_token()));
        let first = save_intent_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=", &pairs(), true).unwrap();
        let mut second_state = latest_state(&first).unwrap();
        second_state.operation_id = crate::flasher::generate_token();
        second_state.sequence += 1;
        let second = root.join(&second_state.operation_id);
        fs::create_dir(&second).unwrap();
        for leaf in CARD_ARTWORK_ASSETS {
            fs::copy(first.join(leaf), second.join(leaf)).unwrap();
        }
        write_new(&second.join("state-0001-prepared.json"), &serde_json::to_vec(&second_state).unwrap()).unwrap();
        assert!(load_latest_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").is_err());

        fs::remove_dir_all(&second).unwrap();
        fs::write(root.join(".create.lock"), b"interrupted process").unwrap();
        assert!(load_latest_at(&root, "device-A", "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
