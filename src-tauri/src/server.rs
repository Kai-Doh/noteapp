use std::sync::Arc;

/// A staged restore (see `backup::engine::stage_restore`) is swapped into the
/// live db path here, before anything else has it open — "never in-place
/// over the live DB while it's open" is satisfied by doing this exactly
/// once, at the very start of boot, rather than as a hot-swap while the
/// writer/read-pool connections are live.
fn apply_pending_restore_if_any() {
    let pending = crate::config::pending_restore_path();
    if !pending.exists() {
        return;
    }
    tracing::info!("applying staged restore from {}", pending.display());
    // Stale WAL/SHM sidecars from the *previous* database would otherwise get
    // replayed against the restored file's contents on next open.
    let _ = std::fs::remove_file(crate::config::wal_path());
    let _ = std::fs::remove_file(crate::config::shm_path());
    std::fs::rename(&pending, crate::config::db_path()).expect("failed to apply staged restore");
}

/// Boots the database, writer thread, token store, and axum server, then
/// serves forever. No Tauri dependency — this is the entire backend, callable
/// both from the headless `server` binary (Docker) and, in principle, from
/// anywhere else that wants an embedded instance. The desktop app itself does
/// *not* call this anymore: it's a thin client that talks to a separately
/// running instance over the network (see `client_config`/`ConnectionSettings`).
pub async fn run() {
    apply_pending_restore_if_any();

    let db_path = crate::config::db_path();
    tracing::info!("opening database at {}", db_path.display());

    let mut migration_conn =
        crate::db::pool::open_write_connection(&db_path).expect("failed to open database");
    crate::db::migrate::run(&mut migration_conn).expect("failed to run migrations");
    drop(migration_conn);

    let write_conn = crate::db::pool::open_write_connection(&db_path)
        .expect("failed to open database for writer");
    let ro_pool =
        crate::db::pool::build_read_pool(&db_path).expect("failed to build read connection pool");
    let writer = crate::db::writer::spawn_writer(write_conn);

    let token_cache = crate::auth::store::load_or_init(&crate::config::token_store_path());

    let state = crate::api::state::AppState {
        writer,
        ro_pool,
        token_cache: Arc::new(token_cache),
    };
    let app_router = crate::api::router::build_router(state);

    crate::backup::spawn_scheduled_backups(crate::config::db_path(), crate::config::backups_dir());

    let addr = crate::config::bind_addr();
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind {addr}: {e}"));
    tracing::info!("API listening on {addr}");
    axum::serve(listener, app_router)
        .await
        .expect("axum server crashed");
}
