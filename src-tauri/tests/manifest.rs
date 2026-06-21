#[test]
fn tauri_dev_uses_app_binary_by_default() {
    let manifest = include_str!("../Cargo.toml");

    assert!(
        manifest.contains("default-run = \"advanced-show-control\""),
        "Cargo must default to the Tauri app binary so `tauri dev` does not fail when lv1-probe is also present"
    );
}
