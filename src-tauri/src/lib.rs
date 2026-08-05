mod api;
mod auth;
mod backup;
#[cfg(feature = "desktop")]
mod client_config;
mod config;
mod db;
mod domain;
mod export;
pub mod server;

// Everything below is Tauri/GUI-specific and only compiled for the desktop
// app — the headless `server` binary is built with `--no-default-features`
// and never touches any of it (see the `desktop` feature in Cargo.toml).
#[cfg(feature = "desktop")]
mod desktop {
    /// What the frontend needs to know about connection state on load —
    /// whether a server URL/token are configured yet, without ever handing
    /// the token itself back through this call (see `get_server_token` for
    /// that).
    #[derive(serde::Serialize)]
    struct ServerConfigDto {
        url: Option<String>,
        has_token: bool,
    }

    #[tauri::command]
    fn get_server_config() -> ServerConfigDto {
        ServerConfigDto {
            url: crate::client_config::load_url(),
            has_token: crate::client_config::load_token().is_some(),
        }
    }

    /// The actual token, for the frontend to attach as a Bearer header on its
    /// own `fetch()` calls to the remote server — those happen in JS, not
    /// proxied through Rust, so the plaintext has to cross this boundary
    /// once per launch.
    #[tauri::command]
    fn get_server_token() -> Option<String> {
        crate::client_config::load_token()
    }

    #[tauri::command]
    fn set_server_config(url: String, token: String) -> Result<(), String> {
        crate::client_config::save(&url, &token).map_err(|e| e.to_string())
    }

    #[tauri::command]
    fn clear_server_config() {
        crate::client_config::clear();
    }

    #[cfg_attr(mobile, tauri::mobile_entry_point)]
    pub fn run() {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
            .init();

        tauri::Builder::default()
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_updater::Builder::new().build())
            .plugin(tauri_plugin_process::init())
            .invoke_handler(tauri::generate_handler![
                get_server_config,
                get_server_token,
                set_server_config,
                clear_server_config
            ])
            .run(tauri::generate_context!())
            .expect("error while running tauri application");
    }
}

#[cfg(feature = "desktop")]
pub use desktop::run;
