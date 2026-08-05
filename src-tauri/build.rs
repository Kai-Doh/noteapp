fn main() {
    // Only the desktop build needs Tauri's codegen — the headless server
    // target is built with `--no-default-features`, which leaves
    // `tauri-build` uncompiled entirely (see the `desktop` feature in
    // Cargo.toml), so this call must not exist in that build at all.
    #[cfg(feature = "desktop")]
    tauri_build::build();
}
