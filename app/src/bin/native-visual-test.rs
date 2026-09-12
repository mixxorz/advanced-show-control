use std::path::PathBuf;

fn main() {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("dist/visual"));
    advanced_show_control::native_ui::visual::run(&output)
        .expect("native visual regression check failed");
}
