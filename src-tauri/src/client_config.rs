use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The desktop app's *own* local settings — which server to talk to and the
/// token to authenticate with. Unrelated to `config.rs`, which configures the
/// server itself; this is purely client-side, since the thin-client desktop
/// app no longer embeds a server of its own.
///
/// The token was originally meant to live in the OS keychain (via the
/// `keyring` crate) rather than here in plaintext — dropped after testing
/// showed `keyring` 3.6.3's Windows backend reports a successful write but
/// then fails to read it back with a fresh `Entry` (reproduced minimally,
/// independent of key names, so it's a crate/platform issue, not something
/// fixable in this codebase). A plaintext local file, protected by normal
/// filesystem permissions, is what most comparable single-user desktop/CLI
/// tools do anyway — reasonable for a personal tool talking to a server on
/// the user's own machine/tailnet.
#[derive(Debug, Serialize, Deserialize, Default)]
struct StoredConfig {
    url: Option<String>,
    token: Option<String>,
}

fn config_path() -> PathBuf {
    let dir = dirs::config_dir()
        .expect("could not resolve OS config directory")
        .join("noteapp");
    std::fs::create_dir_all(&dir).expect("could not create config directory");
    dir.join("client_config.json")
}

fn load() -> StoredConfig {
    std::fs::read_to_string(config_path())
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn load_url() -> Option<String> {
    load().url
}

pub fn load_token() -> Option<String> {
    load().token
}

pub fn save(url: &str, token: &str) -> std::io::Result<()> {
    let cfg = StoredConfig {
        url: Some(url.to_string()),
        token: Some(token.to_string()),
    };
    std::fs::write(
        config_path(),
        serde_json::to_string_pretty(&cfg).expect("serialize client config"),
    )
}

pub fn clear() {
    let _ = std::fs::remove_file(config_path());
}
