//! Headless entry point — no Tauri, no GUI. This is what the Docker image
//! runs: the same backend (`noteapp_lib::server::run`) the desktop app used
//! to embed, now standing alone so multiple clients (the desktop app, an AI
//! agent via the CLI bridge) can share one instance over the network instead
//! of each having their own local copy of the vault.

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    noteapp_lib::server::run().await;
}
