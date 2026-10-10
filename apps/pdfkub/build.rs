//! Windows only: embed the app icon and version info (VERSIONINFO) into `pdfkub.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `PDFKUB_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

#[path = "src/windows_manifest.rs"]
mod windows_manifest;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/pdfkub.ico");
    println!("cargo:rerun-if-changed=src/windows_manifest.rs");
    println!("cargo:rerun-if-env-changed=PDFKUB_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/pdfkub.ico")
        .set_manifest(windows_manifest::WINDOWS_MANIFEST)
        .set("ProductName", "PdfKub")
        .set("FileDescription", "PdfKub PDF workbench")
        .set("LegalCopyright", "Copyright (c) the PdfKub and PdfCraft contributors. MIT OR Apache-2.0.")
        .set("OriginalFilename", "pdfkub.exe")
        .set("InternalName", "pdfkub");
    if let Err(e) = res.compile() {
        if std::env::var_os("PDFKUB_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=pdfkub.exe built without icon/version resources: {e}");
    }
}
