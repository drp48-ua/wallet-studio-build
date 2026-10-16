use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Context, Result};
use regex::Regex;

use crate::device::{ActiveDeviceSession, ConnectionMode};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
unsafe extern "system" {
    fn setsockopt(s: usize, level: i32, optname: i32, optval: *const i8, optlen: i32) -> i32;
}

#[cfg(windows)]
const SOL_SOCKET: i32 = 0xffff;
#[cfg(windows)]
const SO_RCVTIMEO: i32 = 0x1006;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SavedCard {
    pub hash: String,
    pub name: String,
}

pub fn get_cards_storage_path() -> PathBuf {
    let dir = crate::storage::root();
    let _ = fs::create_dir_all(&dir);
    dir.join("cards.json")
}

pub fn is_valid_card_hash(h: &str) -> bool {
    let trimmed = h.trim_matches(['\'', '"']).trim_end_matches(['.', ',']);
    let len = trimmed.len();
    // Real Apple Wallet card hashes are SHA-1 (27-28 chars) or SHA-256 (43-44 chars)
    if len != 27 && len != 28 && len != 43 && len != 44 {
        return false;
    }

    // Must be base64 alphabet characters
    if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '-' || c == '_' || c == '=') {
        return false;
    }

    // Reject strings with multiple underscores or hyphens (typical of system asset/bundle names)
    if trimmed.chars().filter(|&c| c == '_').count() > 1 || trimmed.chars().filter(|&c| c == '-').count() > 2 {
        return false;
    }

    // Reject obvious system identifiers, bundle IDs and common keywords
    let lower = trimmed.to_lowercase();
    if lower.contains("mobileasset")
        || lower.contains("com_apple")
        || lower.contains("com.")
        || lower.contains("apple.")
        || lower.contains("curtain")
        || lower.contains("binder")
        || lower.contains("optimizer")
        || lower.contains("system")
        || lower.contains("uaf")
        || lower.contains("siri")
        || lower.contains("dialog")
        || lower.contains("planner")
        || lower.contains("linguistic")
        || lower.contains("timing")
        || lower.contains("model")
        || lower.contains("translation")
        || lower.contains("visual")
        || lower.contains("device")
        || lower.contains("override")
        || lower.contains("motion")
        || lower.contains("search")
    {
        return false;
    }

    // '=' can only appear at the end
    if let Some(pos) = trimmed.find('=') {
        if pos < len - 2 {
            return false;
        }
    }

    // Normalize URL-safe base64 and pad
    let mut b64 = trimmed.replace('-', "+").replace('_', "/");
    while b64.len() % 4 != 0 {
        b64.push('=');
    }

    use base64::Engine;
    if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(&b64) {
        // Must be exactly 20 bytes (SHA-1) or 32 bytes (SHA-256)
        if decoded.len() == 20 || decoded.len() == 32 {
            // Reject trivial all-identical bytes
            if decoded.iter().all(|&b| b == decoded[0]) {
                return false;
            }

            // Cryptographic hashes have high byte entropy:
            // 1. Must contain both bytes with MSB set (>= 128) and MSB clear (< 128).
            let has_high = decoded.iter().any(|&b| b >= 128);
            let has_low = decoded.iter().any(|&b| b < 128);
            if !has_high || !has_low {
                return false;
            }

            // 2. Must contain at least 12 distinct byte values in 20 bytes
            let mut unique_bytes = std::collections::HashSet::new();
            for &b in &decoded {
                unique_bytes.insert(b);
            }
            if unique_bytes.len() < 12 {
                return false;
            }

            if DUMMY_HASHES.contains(&trimmed) || DUMMY_HASHES.iter().any(|d| d.trim_end_matches('=') == trimmed) {
                return false;
            }
            return true;
        }
    }

    false
}

pub fn load_saved_cards() -> Vec<SavedCard> {
    let path = get_cards_storage_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(cards) = serde_json::from_str::<Vec<SavedCard>>(&content) {
            let valid_cards: Vec<SavedCard> = cards.into_iter().filter(|c| is_valid_card_hash(&c.hash)).collect();
            // Automatically purge corrupted or garbage entries from disk
            save_saved_cards(&valid_cards);
            return valid_cards;
        }
    }
    Vec::new()
}

pub fn save_saved_cards(cards: &[SavedCard]) {
    let path = get_cards_storage_path();
    let _ = write_saved_cards(&path, cards);
}

fn write_saved_cards(path: &std::path::Path, cards: &[SavedCard]) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for c in cards {
        if is_valid_card_hash(&c.hash) && seen.insert(c.hash.clone()) {
            unique.push(c.clone());
        }
    }
    let json = serde_json::to_vec_pretty(&unique)?;
    let temp_path = path.with_extension(format!(
        "json.tmp-{}-{}",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temp_path)?;
        file.write_all(&json)?;
        file.sync_all()?;
    }
    replace_file(&temp_path, path)?;
    Ok(())
}

#[cfg(windows)]
fn replace_file(source: &std::path::Path, destination: &std::path::Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        MoveFileExW(source_wide.as_ptr(), destination_wide.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)
    };
    if result == 0 {
        let error = std::io::Error::last_os_error();
        let _ = fs::remove_file(source);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_file(source: &std::path::Path, destination: &std::path::Path) -> anyhow::Result<()> {
    fs::rename(source, destination)?;
    Ok(())
}

pub fn validate_card_name(name: &str) -> anyhow::Result<String> {
    let name = name.trim();
    anyhow::ensure!(!name.is_empty(), "card name cannot be blank");
    anyhow::ensure!(name.chars().count() <= 48, "card name is longer than 48 characters");
    anyhow::ensure!(!name.chars().any(char::is_control), "card name contains control characters");
    Ok(name.to_owned())
}

pub fn rename_saved_card(hash: &str, name: &str) -> anyhow::Result<()> {
    anyhow::ensure!(is_valid_card_hash(hash), "invalid card identifier");
    let cards = renamed_cards(&load_saved_cards(), hash, name)?;
    write_saved_cards(&get_cards_storage_path(), &cards)
}

fn renamed_cards(cards: &[SavedCard], hash: &str, name: &str) -> anyhow::Result<Vec<SavedCard>> {
    anyhow::ensure!(is_valid_card_hash(hash), "invalid card identifier");
    let name = validate_card_name(name)?;
    let mut updated = cards.to_vec();
    let card = updated.iter_mut().find(|card| card.hash == hash)
        .ok_or_else(|| anyhow::anyhow!("saved card was not found"))?;
    card.name = name;
    Ok(updated)
}

pub fn add_or_update_card(hash: &str, name: &str) {
    if !is_valid_card_hash(hash) {
        return;
    }
    let mut cards = load_saved_cards();
    if let Some(existing) = cards.iter_mut().find(|c| c.hash == hash) {
        if !name.is_empty() && (existing.name.is_empty() || existing.name.starts_with("Card ")) {
            existing.name = name.to_string();
        }
    } else {
        cards.push(SavedCard {
            hash: hash.to_string(),
            name: if name.is_empty() {
                format!("Card {}", cards.len() + 1)
            } else {
                name.to_string()
            },
        });
    }
    save_saved_cards(&cards);
}

const WALLET_KEYWORDS: &[&str] = &[
    "passd",
    "passbook",
    "passkit",
    "stockholm",
    "nanopassd",
    "npkcompanion",
    "wallet",
    "/cards/",
    "/passes/",
];

const DUMMY_HASHES: &[&str] = &[
    "M6nDwZrkYbFlsodLgCbvyFZQ1cc=",
    "kJL-D0rr-SZhbj2c8nK-OQ9hCMY=",
    "hwAtAmHKYwsQrJbT5cTNDsaxVME=",
];

use std::sync::LazyLock;

static DESC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)(?:description|localizedDescription|passName|title)\s*[:=]\s*['"]([^'"]+)['"]"#)
        .unwrap()
});

static CARD_REGEXES: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    vec![
        Regex::new(r"/(?:Cards|Passes/Cards)/([A-Za-z0-9+/_-]{27,44})(?:\.pkpass|\.cache|\.pkcache|/|\s|\x22|'|\)|,|$)").unwrap(),
        Regex::new(r"/([A-Za-z0-9+/_-]{27,44})\.(?:pkpass|cache|pkcache)").unwrap(),
        Regex::new(r"(?:^|[^A-Za-z0-9+/_-])([A-Za-z0-9+/_-]{27}=)(?:$|[^A-Za-z0-9+/_-])").unwrap(),
        Regex::new(r"(?i)(?:card[_\s]?(?:hash|id)|pass[_\s]?(?:hash|id)|unique[_\s]?id)\s*[:=]\s*['\x22]?([A-Za-z0-9+=_-]{27,44})").unwrap(),
    ]
});

pub fn extract_card_name_from_line(line: &str) -> Option<String> {
    if let Some(caps) = DESC_RE.captures(line) {
        if let Some(m) = caps.get(1) {
            let name = m.as_str().trim();
            if name.len() > 1 && !name.to_lowercase().contains("<private>") {
                return Some(name.to_string());
            }
        }
    }
    None
}

pub fn extract_card_hash_from_line(line: &str) -> Option<String> {
    let lower = line.to_lowercase();
    let has_wallet = WALLET_KEYWORDS.iter().any(|k| lower.contains(k));
    if !has_wallet {
        return None;
    }

    for r in CARD_REGEXES.iter() {
        if let Some(caps) = r.captures(line) {
            if let Some(m) = caps.get(1) {
                let h = m.as_str().trim().trim_matches(['\'', '"']).trim_end_matches(['.', ',']);
                if is_valid_card_hash(h) {
                    let mut norm = h.to_string();
                    if norm.len() == 27 {
                        norm.push('=');
                    }
                    return Some(norm);
                }
            }
        }
    }

    None
}

/// One user-requested scan selects one exact card, then releases the relay.
/// Bounded line storage also prevents a malformed syslog stream growing memory.
fn receive_first_card(stop:&AtomicBool,mut receive:impl FnMut(&mut [u8])->isize)->Option<(String,String)> {
    let mut buffer=[0u8;8192];let mut line=Vec::with_capacity(1024);let mut oversized=false;
    while !stop.load(Ordering::Acquire) {
        let count=receive(&mut buffer);
        if count==0 {break;}
        if count<0 {std::thread::sleep(std::time::Duration::from_millis(50));continue;}
        if count as usize>buffer.len() {break;}
        for &byte in &buffer[..count as usize] {
            if stop.load(Ordering::Acquire) {return None;}
            if byte==b'\n' || byte==0 {
                if !oversized && !line.is_empty() {
                    let text=String::from_utf8_lossy(&line);
                    if let Some(hash)=extract_card_hash_from_line(&text).filter(|hash|crate::wallet_export::is_exact_card_hash(hash)) {
                        let name=extract_card_name_from_line(&text).unwrap_or_default();
                        stop.store(true,Ordering::Release);return Some((hash,name));
                    }
                }
                line.clear();oversized=false;
            } else if byte!=b'\r' && !oversized {
                if line.len()<16384 {line.push(byte);} else {line.clear();oversized=true;}
            }
        }
    } None
}

pub fn scan_syslog_for_cards<F, L>(
    udid: Option<&str>,
    connection_mode: ConnectionMode,
    stop_flag: Arc<AtomicBool>,
    mut on_card_found: F,
    mut log: L,
) -> Result<()>
where
    F: FnMut(String, String),
    L: FnMut(String),
{
    log("Connecting to device session for syslog monitoring...".to_string());
    let session = ActiveDeviceSession::open(udid, connection_mode)
        .context("Failed to connect to device for syslog scanning")?;
    log(format!(
        "Connected to {} over {}.",
        session.udid,
        session.transport.label()
    ));
    let libs = &session.libs;
    log("Starting com.apple.syslog_relay service on device...".to_string());
    let service_conn = session.start_service("com.apple.syslog_relay")
        .context("Failed to start com.apple.syslog_relay service")?;

    let raw_socket = unsafe { (libs.amd_service_connection_get_socket)(service_conn) };
    if raw_socket <= 0 {
        unsafe { (libs.amd_service_connection_invalidate)(service_conn) };
        anyhow::bail!("Invalid syslog socket");
    }

    // Set socket receive timeout
    #[cfg(windows)]
    unsafe {
        let timeout_ms: u32 = 500;
        let result=setsockopt(
            raw_socket as usize,
            SOL_SOCKET,
            SO_RCVTIMEO,
            &timeout_ms as *const u32 as *const i8,
            std::mem::size_of::<u32>() as i32,
        );
        if result!=0 {
            (libs.amd_service_connection_invalidate)(service_conn);
            anyhow::bail!("No se pudo configurar el tiempo de espera del receptor");
        }
    }

    log("Syslog relay established. Listening for Wallet & PassKit events...".to_string());
    log("Tip: Open Apple Wallet on your iPhone or tap your card to trigger events.".to_string());

    let found=receive_first_card(&stop_flag,|buffer| unsafe {
        (libs.amd_service_connection_receive)(service_conn,buffer.as_mut_ptr(),buffer.len()) as isize
    });
    // Release both the relay and the device session BEFORE notifying the UI.
    // The next task (original capture) must never compete with this receiver.
    unsafe { (libs.amd_service_connection_invalidate)(service_conn); }
    drop(session);
    if let Some((hash,name))=found {
        if !load_saved_cards().iter().any(|card|card.hash==hash) {add_or_update_card(&hash,&name);}
        on_card_found(hash,name);
    }
    log("Syslog scan stopped; relay and device session released.".to_string());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_stops_after_one_exact_card_even_when_same_receive_contains_more() {
        let stop=AtomicBool::new(false);let mut receives=0;
        let bytes=b"passd: Card hash: 'OM6NYhwXMZrAw0sRUjR62wmF4ZQ=' title='Mi tarjeta' loaded\npassd: Card hash: 'OM6NYhwXMZrAw0sRUjR62wmF4ZQ=' loaded\n";
        let found=receive_first_card(&stop,|buffer| {receives+=1;buffer[..bytes.len()].copy_from_slice(bytes);bytes.len() as isize});
        assert_eq!(found,Some(("OM6NYhwXMZrAw0sRUjR62wmF4ZQ=".into(),"Mi tarjeta".into())));
        assert!(stop.load(Ordering::Acquire));assert_eq!(receives,1);
    }
    #[test]
    fn cancelled_scan_never_receives_or_reports_a_card() {
        let stop=AtomicBool::new(true);
        assert_eq!(receive_first_card(&stop,|_|panic!("cancelled receiver must not run")),None);
    }
    #[test]
    fn partial_and_invalid_syslog_lines_do_not_finish_scan() {
        let stop=AtomicBool::new(false);let mut count=0;
        let parts:Vec<&[u8]>=vec![b"passd: Card hash: 'not-a-card'\npassd: Card hash: 'OM6NYhwXMZrA",b"w0sRUjR62wmF4ZQ=' loaded\n"];
        let found=receive_first_card(&stop,|buffer| {let part=parts[count];count+=1;buffer[..part.len()].copy_from_slice(part);part.len() as isize});
        assert!(found.is_some());assert_eq!(count,2);
    }

    #[test]
    fn test_extract_card_hash() {
        let line1 = "passd[123]: Card hash: 'OM6NYhwXMZrAw0sRUjR62wmF4ZQ=' loaded";
        assert_eq!(
            extract_card_hash_from_line(line1),
            Some("OM6NYhwXMZrAw0sRUjR62wmF4ZQ=".to_string())
        );

        let line2 = "nanopassd: Accessing /var/mobile/Library/Passes/Cards/d64fKk0kyHWP11IWV2GRLud4XQk.pkpass";
        assert_eq!(
            extract_card_hash_from_line(line2),
            Some("d64fKk0kyHWP11IWV2GRLud4XQk=".to_string())
        );

        // Dummy/unrelated lines should be ignored
        let dummy = "passd: Using dummy hash hwAtAmHKYwsQrJbT5cTNDsaxVME=";
        assert_eq!(extract_card_hash_from_line(dummy), None);
    }

    #[test]
    fn test_extract_card_name() {
        let line = "passd[456]: Pass with localizedDescription = 'Apple Card' updated";
        assert_eq!(
            extract_card_name_from_line(line),
            Some("Apple Card".to_string())
        );
    }

    #[test]
    fn test_is_valid_card_hash() {
        // Real card hashes
        assert!(is_valid_card_hash("OM6NYhwXMZrAw0sRUjR62wmF4ZQ="));
        assert!(is_valid_card_hash("d64fKk0kyHWP11IWV2GRLud4XQk="));
        assert!(is_valid_card_hash("d64fKk0kyHWP11IWV2GRLud4XQk"));

        // System garbage strings that must be rejected
        let garbage = [
            "PresentationBinderIndirectAccessHosting-",
            "SB-systemApertureCurtain",
            "com_apple_MobileAsset_UAF_Translation_Assets",
            "com_apple_MobileAsset_UAF_FM_Visual",
            "com_apple_MobileAsset_UAF_DeviceCheck",
            "com_apple_MobileAsset_UAF_Siri_TextToSpeech",
            "com_apple_MobileAsset_UAF_Siri_DialogAssets",
            "com_apple_MobileAsset_UAF_IF_Planner",
            "SubscriptionOptimizerTimingModels",
            "com_apple_MobileAsset_UAF_LinguisticData",
            "com_apple_MobileAsset_UAF_FM_Overrides",
            "com_apple_MobileAsset_UAF_Siri_Understanding",
            "com_apple_MobileAsset_UAF_MotionAnomalyFM",
            "com_apple_MobileAsset_UAF_TKModelMessages",
            "com_apple_MobileAsset_UAF_Search_ODLA",
        ];

        for g in garbage {
            assert!(!is_valid_card_hash(g), "Expected {} to be rejected as card hash", g);
        }
    }

    #[test]
    fn rename_preserves_hash_and_persists_name_outside_user_storage() {
        let hash = "OM6NYhwXMZrAw0sRUjR62wmF4ZQ=";
        let original = vec![SavedCard {
            hash: hash.to_string(),
            name: "Card 1".to_string(),
        }];
        let renamed = renamed_cards(&original, hash, "  Transit card  ").unwrap();
        assert_eq!(renamed[0].hash, hash);
        assert_eq!(renamed[0].name, "Transit card");

        let path = std::env::temp_dir().join(format!(
            "wallet-studio-card-names-{}-{}.json",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        write_saved_cards(&path, &renamed).unwrap();
        let reloaded: Vec<SavedCard> = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(reloaded, renamed);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn card_name_rejects_blank_control_and_too_long_values() {
        assert!(validate_card_name(" \t ").is_err());
        assert!(validate_card_name("bad\nname").is_err());
        assert!(validate_card_name(&"x".repeat(49)).is_err());
        assert_eq!(validate_card_name("  Visa  ").unwrap(), "Visa");
    }

    #[test]
    fn test_saved_cards_purging() {
        let loaded = load_saved_cards();
        for card in &loaded {
            assert!(is_valid_card_hash(&card.hash), "Invalid hash was not purged: {}", card.hash);
        }
    }

    #[test]
    fn test_syslog_service_receive() {
        let session = match ActiveDeviceSession::open(None, ConnectionMode::Auto) {
            Ok(s) => s,
            Err(e) => {
                println!("No device connected: {:?}", e);
                return;
            }
        };
        let libs = &session.libs;
        let conn = session.start_service("com.apple.syslog_relay").expect("start syslog_relay");
        let raw_socket = unsafe { (libs.amd_service_connection_get_socket)(conn) };
        unsafe {
            let timeout_ms: u32 = 500;
            setsockopt(
                raw_socket as usize,
                SOL_SOCKET,
                SO_RCVTIMEO,
                &timeout_ms as *const u32 as *const i8,
                std::mem::size_of::<u32>() as i32,
            );
        }
        let mut buf = [0u8; 4096];
        let start = std::time::Instant::now();
        let n = unsafe { (libs.amd_service_connection_receive)(conn, buf.as_mut_ptr(), buf.len()) };
        println!("AMDServiceConnectionReceive returned: {} in {:?}", n, start.elapsed());
        unsafe { (libs.amd_service_connection_invalidate)(conn) };
    }
}
