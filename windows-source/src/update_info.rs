//! Public distribution is a release-time choice, never a credentialed updater.
pub const VERSION: &str = "0.5.4";
pub const PUBLIC_DOWNLOAD_URL: &str = "https://github.com/drp48-ua/wallet-studio-releases/releases/latest";
pub const PUBLIC_CHANNEL_PUBLISHED: bool = false;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_updates_use_a_fixed_https_channel_without_credentials() {
        assert_eq!(PUBLIC_DOWNLOAD_URL, "https://github.com/drp48-ua/wallet-studio-releases/releases/latest");
        assert!(!PUBLIC_DOWNLOAD_URL.contains('@'));
        assert!(!PUBLIC_DOWNLOAD_URL.contains('?'));
        assert!(!PUBLIC_CHANNEL_PUBLISHED);
    }
}
