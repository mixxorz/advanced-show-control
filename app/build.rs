fn main() {
    for (name, fallback) in [
        ("ASC_RELEASE_ID", "local"),
        ("ASC_RELEASE_VERSION", env!("CARGO_PKG_VERSION")),
    ] {
        println!("cargo:rerun-if-env-changed={name}");
        let value = std::env::var(name).unwrap_or_else(|_| fallback.to_string());
        println!("cargo:rustc-env={name}={value}");
    }
    #[cfg(target_os = "windows")]
    {
        winresource::WindowsResource::new()
            .set_icon("icons/icon.ico")
            .set("ProductName", "Advanced Show Control")
            .set("FileDescription", "Advanced Show Control")
            .set("LegalCopyright", "GPL-3.0-or-later")
            .compile()
            .expect("failed to embed Windows application resources");
    }
}
