#[test]
fn tauri_dev_uses_app_binary_by_default() {
    let manifest = include_str!("../Cargo.toml");

    assert!(
        manifest.contains("default-run = \"advanced-show-control\""),
        "Cargo must default to the Tauri app binary so `tauri dev` does not fail when lv1-probe is also present"
    );
}

#[test]
fn debug_app_binary_is_declared_by_source_file() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/bin/advanced-show-control-debug.rs");

    assert!(path.exists(), "debug app binary source file must exist");
}
