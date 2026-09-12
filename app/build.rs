fn main() {
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
