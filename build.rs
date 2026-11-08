// Windows: embed the icon and version info into hyve.exe.
// `native_window`: the desktop feature on Windows / macOS (wry + tao). Linux shows a browser app window.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    println!("cargo::rustc-check-cfg=cfg(native_window)");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_some() && os != "linux" {
        println!("cargo:rustc-cfg=native_window");
    }
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "Hyve")
            .set("FileDescription", "Hyve - Hytale & Minecraft servers")
            .set("CompanyName", "Akulav")
            .set("LegalCopyright", "Made by Akulav");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the Windows icon: {e}");
        }
    }
}
