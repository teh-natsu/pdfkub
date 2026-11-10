//! Windows only: embed the app icon and version info (VERSIONINFO) into `pdfkub.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab. A separate PDF document icon
//! uses resource ID 2; the installer references it as `pdfkub.exe,-2`.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `PDFKUB_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

#[path = "src/windows_manifest.rs"]
mod windows_manifest;

/// The in-box DLLs only printing uses (Windows.Data.Pdf draws the job, prntvpt reads the
/// driver's print capabilities). Trimmed Windows installations (NTLite) can lack them, and a
/// load-time import of a missing DLL stops the program before `main` (#2630). Delay-loaded, they
/// load on the first print; `pdfcraft_print::spool` checks they exist before calling in.
/// Same list in `apps/pdfkub{,-cli}/build.rs`, `crates/print/build.rs` and `crates/print/src/spool.rs`.
const DELAY_LOADED: [&str; 2] = ["windows.data.pdf.dll", "prntvpt.dll"];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/pdfkub.ico");
    println!("cargo:rerun-if-changed=../../assets/app-icon/pdfkub-document.ico");
    println!("cargo:rerun-if-changed=src/windows_manifest.rs");
    println!("cargo:rerun-if-env-changed=PDFKUB_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        for dll in DELAY_LOADED {
            println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
        }
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/pdfkub.ico")
        .set_icon_with_id("../../assets/app-icon/pdfkub-document.ico", "2")
        .set_manifest(windows_manifest::WINDOWS_MANIFEST)
        .set("ProductName", "PdfKub")
        .set("FileDescription", "PdfKub PDF workbench")
        .set("CompanyName", "Nattapol Chaisri")
        .set("LegalCopyright", "Copyright (c) 2026 Nattapol Chaisri and the PdfKub contributors; based on PdfCraft. MIT OR Apache-2.0.")
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
