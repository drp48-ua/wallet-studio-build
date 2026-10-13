//! Strict parsing and verification for Apple's remote Wallet artwork manifest.
//! This module handles only declared artwork URLs and never reads pass data.

use anyhow::{Context, Result};
use std::io::Cursor;
use std::time::{Duration, Instant};

pub const MAX_ARTWORK_BYTES: usize = 32 * 1024 * 1024;
const DOWNLOAD_TIMEOUT_MS: i32 = 45_000;
const APPLE_SUFFIX: &str = ".apple.com";
const ALLOWED_NAMES: &[&str] = &[
    "cardBackgroundCombined@3x.png",
    "cardBackgroundCombined@2x.png",
    "cardBackgroundCombined.png",
    "cardBackgroundCombined.pdf",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredArtwork {
    pub name: String,
    pub url: String,
    pub size: usize,
    /// Lowercase SHA-1 hex from the manifest.
    pub sha1: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedUrl {
    host: String,
    path_and_query: String,
}

fn parse_url(url: &str) -> Result<ParsedUrl> {
    anyhow::ensure!(url.is_ascii() && !url.bytes().any(|b| b <= 0x20 || b == 0x7f),
        "Artwork URL contains whitespace or non-ASCII characters");
    anyhow::ensure!(!url.contains(['\\', '#']), "Artwork URL contains a forbidden delimiter");
    let rest = url.strip_prefix("https://").context("Artwork URL must use HTTPS")?;
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let suffix = &rest[authority_end..];
    anyhow::ensure!(!authority.is_empty() && !authority.contains('@'),
        "Artwork URL must not contain credentials");
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => {
            anyhow::ensure!(!host.contains(':') && port == "443", "Artwork URL uses a nonstandard port");
            (host, Some(port))
        }
        None => (authority, None),
    };
    let _ = port;
    let host = host.to_ascii_lowercase();
    anyhow::ensure!(host == "apple.com" || host.ends_with(APPLE_SUFFIX),
        "Artwork host must be apple.com or one of its subdomains");
    anyhow::ensure!(host.len() <= 253 && host.split('.').all(valid_dns_label),
        "Artwork URL has an invalid DNS hostname");
    let path_and_query = if suffix.is_empty() { "/".to_string() }
        else if suffix.starts_with('?') { format!("/{suffix}") }
        else { suffix.to_string() };
    anyhow::ensure!(path_and_query.bytes().all(|b| (0x21..=0x7e).contains(&b)),
        "Artwork URL path contains invalid bytes");
    Ok(ParsedUrl { host, path_and_query })
}

fn valid_dns_label(label: &str) -> bool {
    !label.is_empty() && label.len() <= 63
        && label.as_bytes()[0].is_ascii_alphanumeric()
        && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
        && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn parse_sha1(value: &str) -> Result<String> {
    anyhow::ensure!(value.len() == 40 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        "Artwork manifest SHA-1 must be exactly 40 hexadecimal characters");
    Ok(value.to_ascii_lowercase())
}

fn parse_entry(name: &str, url: &str, size: u64, sha1: &str) -> Result<DeclaredArtwork> {
    anyhow::ensure!(ALLOWED_NAMES.contains(&name), "Manifest entry is not a supported card artwork image");
    anyhow::ensure!(size > 0 && size <= MAX_ARTWORK_BYTES as u64,
        "Declared artwork size is outside the allowed range");
    parse_url(url)?;
    Ok(DeclaredArtwork { name: name.to_string(), url: url.to_string(),
        size: usize::try_from(size)?, sha1: parse_sha1(sha1)? })
}

/// Parse JSON or binary/XML plist manifests. Every recognized image entry must
/// carry both size and SHA-1; unsupported/non-image keys are ignored.
pub fn parse_manifest(bytes: &[u8]) -> Result<Vec<DeclaredArtwork>> {
    anyhow::ensure!(!bytes.is_empty() && bytes.len() <= MAX_ARTWORK_BYTES,
        "Artwork manifest is empty or too large");
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) {
        return parse_json_manifest(&value);
    }
    let value = plist::Value::from_reader(Cursor::new(bytes)).context("Artwork manifest is neither valid JSON nor plist")?;
    parse_plist_manifest(&value)
}

fn parse_json_manifest(value: &serde_json::Value) -> Result<Vec<DeclaredArtwork>> {
    let object = value.as_object().context("Artwork manifest root must be an object")?;
    let mut entries = Vec::new();
    for (name, metadata) in object {
        if !ALLOWED_NAMES.contains(&name.as_str()) { continue; }
        let metadata = metadata.as_object().context("Artwork metadata must be an object")?;
        let url = metadata.get("url").and_then(serde_json::Value::as_str)
            .context("Artwork entry is missing a string URL")?;
        let size = metadata.get("size").and_then(serde_json::Value::as_u64)
            .context("Artwork entry is missing a positive integer size")?;
        let sha1 = metadata.get("sha1").and_then(serde_json::Value::as_str)
            .context("Artwork entry is missing a SHA-1")?;
        entries.push(parse_entry(name, url, size, sha1)?);
    }
    anyhow::ensure!(!entries.is_empty(), "Manifest contains no supported, verifiable artwork entries");
    order_entries(entries)
}

fn parse_plist_manifest(value: &plist::Value) -> Result<Vec<DeclaredArtwork>> {
    let object = value.as_dictionary().context("Artwork manifest root must be a dictionary")?;
    let mut entries = Vec::new();
    for (name, metadata) in object {
        if !ALLOWED_NAMES.contains(&name.as_str()) { continue; }
        let metadata = metadata.as_dictionary().context("Artwork metadata must be a dictionary")?;
        let url = metadata.get("url").and_then(plist::Value::as_string)
            .context("Artwork entry is missing a string URL")?;
        let size = metadata.get("size").and_then(plist::Value::as_unsigned_integer)
            .context("Artwork entry is missing a positive integer size")?;
        let sha1 = metadata.get("sha1").and_then(plist::Value::as_string)
            .context("Artwork entry is missing a SHA-1")?;
        entries.push(parse_entry(name, url, size, sha1)?);
    }
    anyhow::ensure!(!entries.is_empty(), "Manifest contains no supported, verifiable artwork entries");
    order_entries(entries)
}

fn order_entries(mut entries: Vec<DeclaredArtwork>) -> Result<Vec<DeclaredArtwork>> {
    const PREFERENCE: &[&str] = &[
        "cardBackgroundCombined@3x.png", "cardBackgroundCombined@2x.png",
        "cardBackgroundCombined.png", "cardBackgroundCombined.pdf",
    ];
    entries.sort_by_key(|entry| PREFERENCE.iter().position(|name| *name == entry.name).unwrap_or(usize::MAX));
    Ok(entries)
}

fn validate_status(status: u32) -> Result<()> {
    anyhow::ensure!(status == 200, "Artwork server returned HTTP status {status}; redirects are disabled");
    Ok(())
}

/// Download one declared asset, rejecting redirects and verifying exact size
/// and SHA-1 before returning any bytes to the caller.
pub fn download_verified_artwork(artwork: &DeclaredArtwork) -> Result<Vec<u8>> {
    anyhow::ensure!(ALLOWED_NAMES.contains(&artwork.name.as_str()), "Unsupported artwork name");
    anyhow::ensure!(artwork.size > 0 && artwork.size <= MAX_ARTWORK_BYTES,
        "Declared artwork size is outside the allowed range");
    let parsed = parse_url(&artwork.url)?;
    let deadline = Instant::now() + Duration::from_millis(DOWNLOAD_TIMEOUT_MS as u64);
    let bytes = winhttp_get(&parsed, deadline)?;
    verify_payload(artwork, &bytes)?;
    anyhow::ensure!(Instant::now() <= deadline,
        "Artwork download and verification exceeded the 45-second total deadline");
    Ok(bytes)
}

fn verify_payload(artwork: &DeclaredArtwork, bytes: &[u8]) -> Result<()> {
    anyhow::ensure!(bytes.len() <= MAX_ARTWORK_BYTES, "Artwork response exceeded the 32 MiB limit");
    anyhow::ensure!(bytes.len() == artwork.size, "Downloaded artwork size does not match the manifest");
    anyhow::ensure!(sha1_hex(bytes)? == parse_sha1(&artwork.sha1)?,
        "Downloaded artwork SHA-1 does not match the manifest");
    validate_artwork_format(bytes)?;
    Ok(())
}

fn validate_artwork_format(bytes: &[u8]) -> Result<()> {
    if bytes.starts_with(b"%PDF-") {
        anyhow::ensure!(bytes.windows(5).any(|window| window == b"%%EOF"),
            "PDF artwork has no end-of-file marker");
        return Ok(());
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format().context("Artwork format could not be identified")?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8_192);
    limits.max_image_height = Some(8_192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().context("Artwork image could not be fully decoded")?;
    anyhow::ensure!(decoded.width() > 0 && decoded.height() > 0,
        "Decoded artwork has invalid dimensions");
    Ok(())
}

fn sha1_hex(bytes: &[u8]) -> Result<String> {
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptOpenAlgorithmProvider(handle: *mut *mut std::ffi::c_void, name: *const u16,
            implementation: *const u16, flags: u32) -> i32;
        fn BCryptHash(handle: *mut std::ffi::c_void, secret: *const u8, secret_len: u32,
            input: *const u8, input_len: u32, output: *mut u8, output_len: u32) -> i32;
        fn BCryptCloseAlgorithmProvider(handle: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    let name: Vec<u16> = "SHA1\0".encode_utf16().collect();
    let mut algorithm = std::ptr::null_mut();
    let mut digest = [0u8; 20];
    unsafe {
        anyhow::ensure!(BCryptOpenAlgorithmProvider(&mut algorithm, name.as_ptr(), std::ptr::null(), 0) == 0,
            "Could not initialize SHA-1 verification");
        let status = BCryptHash(algorithm, std::ptr::null(), 0, bytes.as_ptr(),
            u32::try_from(bytes.len())?, digest.as_mut_ptr(), digest.len() as u32);
        BCryptCloseAlgorithmProvider(algorithm, 0);
        anyhow::ensure!(status == 0, "Could not compute artwork SHA-1");
    }
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

struct WinHandle(*mut std::ffi::c_void);
impl Drop for WinHandle { fn drop(&mut self) { unsafe { WinHttpCloseHandle(self.0); } } }

#[link(name = "winhttp")]
unsafe extern "system" {
    fn WinHttpOpen(agent: *const u16, access_type: u32, proxy: *const u16,
        bypass: *const u16, flags: u32) -> *mut std::ffi::c_void;
    fn WinHttpConnect(session: *mut std::ffi::c_void, server: *const u16,
        port: u16, reserved: u32) -> *mut std::ffi::c_void;
    fn WinHttpOpenRequest(connection: *mut std::ffi::c_void, verb: *const u16,
        object: *const u16, version: *const u16, referrer: *const u16,
        accept_types: *const *const u16, flags: u32) -> *mut std::ffi::c_void;
    fn WinHttpSetOption(handle: *mut std::ffi::c_void, option: u32,
        buffer: *mut std::ffi::c_void, length: u32) -> i32;
    fn WinHttpSetTimeouts(session: *mut std::ffi::c_void, resolve: i32,
        connect: i32, send: i32, receive: i32) -> i32;
    fn WinHttpSendRequest(request: *mut std::ffi::c_void, headers: *const u16,
        header_length: u32, optional: *mut std::ffi::c_void, optional_length: u32,
        total_length: u32, context: usize) -> i32;
    fn WinHttpReceiveResponse(request: *mut std::ffi::c_void, reserved: *mut std::ffi::c_void) -> i32;
    fn WinHttpQueryHeaders(request: *mut std::ffi::c_void, info: u32,
        name: *const u16, buffer: *mut std::ffi::c_void, length: *mut u32,
        index: *mut u32) -> i32;
    fn WinHttpReadData(request: *mut std::ffi::c_void, buffer: *mut std::ffi::c_void,
        bytes_to_read: u32, bytes_read: *mut u32) -> i32;
    fn WinHttpCloseHandle(handle: *mut std::ffi::c_void) -> i32;
}

fn winhttp_get(url: &ParsedUrl, deadline: Instant) -> Result<Vec<u8>> {
    const NO_PROXY: u32 = 1;
    const SECURE: u32 = 0x0080_0000;
    const DISABLE_FEATURE: u32 = 63;
    const DISABLE_REDIRECTS: u32 = 0x0000_0008;
    const QUERY_STATUS_CODE: u32 = 19 | 0x2000_0000;
    let agent: Vec<u16> = "Wallet Studio Artwork\0".encode_utf16().collect();
    let host: Vec<u16> = format!("{}\0", url.host).encode_utf16().collect();
    let path: Vec<u16> = format!("{}\0", url.path_and_query).encode_utf16().collect();
    let get: Vec<u16> = "GET\0".encode_utf16().collect();
    let session = WinHandle(unsafe { WinHttpOpen(agent.as_ptr(), NO_PROXY,
        std::ptr::null(), std::ptr::null(), 0) });
    anyhow::ensure!(!session.0.is_null(), "WinHTTP session initialization failed: {}", std::io::Error::last_os_error());
    set_remaining_timeout(session.0, deadline, true)?;
    let connection = WinHandle(unsafe { WinHttpConnect(session.0, host.as_ptr(), 443, 0) });
    anyhow::ensure!(!connection.0.is_null(), "WinHTTP connection failed: {}", std::io::Error::last_os_error());
    let request = WinHandle(unsafe { WinHttpOpenRequest(connection.0, get.as_ptr(), path.as_ptr(),
        std::ptr::null(), std::ptr::null(), std::ptr::null(), SECURE) });
    anyhow::ensure!(!request.0.is_null(), "WinHTTP request creation failed: {}", std::io::Error::last_os_error());
    let mut disabled = DISABLE_REDIRECTS;
    anyhow::ensure!(unsafe { WinHttpSetOption(request.0, DISABLE_FEATURE,
        (&mut disabled as *mut u32).cast(), std::mem::size_of::<u32>() as u32) } != 0,
        "Could not disable HTTP redirects");
    set_remaining_timeout(session.0, deadline, false)?;
    anyhow::ensure!(unsafe { WinHttpSendRequest(request.0, std::ptr::null(), 0,
        std::ptr::null_mut(), 0, 0, 0) } != 0, "WinHTTP send failed: {}", std::io::Error::last_os_error());
    set_remaining_timeout(session.0, deadline, false)?;
    anyhow::ensure!(unsafe { WinHttpReceiveResponse(request.0, std::ptr::null_mut()) } != 0,
        "WinHTTP response failed: {}", std::io::Error::last_os_error());
    let mut status = 0u32;
    let mut status_len = std::mem::size_of::<u32>() as u32;
    anyhow::ensure!(unsafe { WinHttpQueryHeaders(request.0, QUERY_STATUS_CODE, std::ptr::null(),
        (&mut status as *mut u32).cast(), &mut status_len, std::ptr::null_mut()) } != 0,
        "Could not read HTTP status");
    validate_status(status)?;

    let mut result = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        set_remaining_timeout(session.0, deadline, false)?;
        let mut read = 0u32;
        anyhow::ensure!(unsafe { WinHttpReadData(request.0, buffer.as_mut_ptr().cast(),
            buffer.len() as u32, &mut read) } != 0, "WinHTTP response read failed: {}", std::io::Error::last_os_error());
        if read == 0 { break; }
        anyhow::ensure!(result.len().saturating_add(read as usize) <= MAX_ARTWORK_BYTES,
            "Artwork response exceeded the 32 MiB limit");
        result.extend_from_slice(&buffer[..read as usize]);
    }
    anyhow::ensure!(Instant::now() <= deadline, "Artwork download exceeded its 45-second total deadline");
    Ok(result)
}

fn set_remaining_timeout(session: *mut std::ffi::c_void, deadline: Instant, split_connect: bool) -> Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now()).as_millis();
    anyhow::ensure!(remaining > 0, "Artwork download exceeded its 45-second total deadline");
    let timeout = i32::try_from(remaining.min(i32::MAX as u128))?;
    let (resolve, connect) = if split_connect { (timeout / 2, timeout / 2) } else { (timeout, timeout) };
    anyhow::ensure!(unsafe { WinHttpSetTimeouts(session, resolve.max(1), connect.max(1),
        timeout, timeout) } != 0, "Could not update remaining WinHTTP deadline");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

    fn json_for(url: &str, size: usize, sha1: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "cardBackgroundCombined@2x.png": {"url": url, "size": size, "sha1": sha1}
        })).unwrap()
    }

    #[test]
    fn parses_supported_entry_and_orders_preferred_scale_first() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "cardBackgroundCombined.pdf": {"url":"https://a.apple.com/x", "size":3, "sha1":HASH},
            "cardBackgroundCombined@2x.png": {"url":"https://b.apple.com/y", "size":3, "sha1":HASH},
            "not-artwork": {"url":"https://evil.example/x", "size":3, "sha1":HASH}
        })).unwrap();
        let parsed = parse_manifest(&bytes).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "cardBackgroundCombined@2x.png");
    }

    #[test]
    fn rejects_missing_size_hash_and_unsupported_asset_names() {
        assert!(parse_manifest(br#"{"cardBackgroundCombined@2x.png":{"url":"https://a.apple.com/x","size":3}}"#).is_err());
        assert!(parse_manifest(br#"{"cardBackgroundCombined@2x.png":{"url":"https://a.apple.com/x","sha1":"a9993e364706816aba3e25717850c26c9cd0d89d"}}"#).is_err());
        assert!(parse_manifest(&json_for("https://a.apple.com/x", 3, HASH)).is_ok());
        let unsupported = serde_json::to_vec(&serde_json::json!({"pass.json":{"url":"https://a.apple.com/x","size":3,"sha1":HASH}})).unwrap();
        assert!(parse_manifest(&unsupported).is_err());
    }

    #[test]
    fn validates_https_host_credentials_and_port() {
        for valid in ["https://apple.com/x", "https://pr-pod11.assets.apple.com/a"] {
            assert!(parse_url(valid).is_ok(), "{valid}");
        }
        for invalid in ["http://a.apple.com/x", "https://apple.com.evil.test/x",
            "https://evilapple.com/x", "https://user@a.apple.com/x",
            "https://a.apple.com:444/x", "https://a.apple.com:443:443/x"] {
            assert!(parse_url(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn redirects_are_rejected_and_only_success_status_is_accepted() {
        assert!(validate_status(200).is_ok());
        for redirect in [301, 302, 307, 308] { assert!(validate_status(redirect).is_err()); }
        assert!(validate_status(404).is_err());
    }

    #[test]
    fn verifies_size_hash_and_response_bound_without_network() {
        let mut bytes = Vec::new();
        image::DynamicImage::new_rgb8(2, 2).write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        let digest = sha1_hex(&bytes).unwrap();
        assert_eq!(parse_sha1(HASH).unwrap(), HASH);
        assert!(parse_sha1("not-sha1").is_err());
        let entry = DeclaredArtwork { name: "cardBackgroundCombined@2x.png".into(),
            url: "https://a.apple.com/x".into(), size: bytes.len(), sha1: digest };
        assert!(verify_payload(&entry, &bytes).is_ok());
        assert!(verify_payload(&entry, b"\x89PNG\r\n\x1a\nxyz").is_err());
        let wrong_size = DeclaredArtwork { size: bytes.len() + 1, ..entry.clone() };
        assert!(verify_payload(&wrong_size, &bytes).is_err());
        let bad_hash = DeclaredArtwork { sha1: HASH.into(), ..entry };
        assert!(verify_payload(&bad_hash, &bytes).is_err());
        let too_large = DeclaredArtwork { name: "cardBackgroundCombined@2x.png".into(),
            url: "https://a.apple.com/x".into(), size: MAX_ARTWORK_BYTES + 1, sha1: HASH.into() };
        assert!(verify_payload(&too_large, &[]).is_err());
        assert!(parse_manifest(&json_for("https://a.apple.com/x", MAX_ARTWORK_BYTES + 1, HASH)).is_err());
        assert!(validate_artwork_format(b"abc").is_err());
        assert!(validate_artwork_format(b"%PDF-1.7\n%%EOF").is_ok());
        assert!(validate_artwork_format(b"%PDF-1.7\ntruncated").is_err());
    }
}
