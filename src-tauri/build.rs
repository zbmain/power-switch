/// Generate the desktop bundle metadata only when building the Tauri shell.
fn main() {
    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() {
        tauri_build::build();
    }
}
