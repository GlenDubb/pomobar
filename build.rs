//! Embeds the app icon, the Windows application manifest (per-monitor DPI
//! awareness, modern common controls) and version information into the .exe.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=res/pomobar.ico");
    println!("cargo:rerun-if-changed=res/app.manifest");
    println!("cargo:rerun-if-changed=build.rs");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Type-checking from Linux/macOS has no resource compiler; real builds run on Windows.
    if !cfg!(windows) && env::var_os("POMOBAR_FORCE_RESOURCES").is_none() {
        println!("cargo:warning=Skipping Windows resources (not building on Windows)");
        return;
    }

    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let mut parts = version.split(['.', '-']).map(|p| p.parse::<u16>().unwrap_or(0));
    let (major, minor, patch) = (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    );

    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let path = |name: &str| root.join("res").join(name).display().to_string().replace('\\', "/");

    let rc = format!(
        r#"1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Williams Automation"
      VALUE "FileDescription", "WillPomo"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "pomobar"
      VALUE "LegalCopyright", "Copyright (c) 2026 Williams Automation. MIT License."
      VALUE "OriginalFilename", "pomobar.exe"
      VALUE "ProductName", "WillPomo"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = path("pomobar.ico"),
        manifest = path("app.manifest"),
    );

    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("pomobar.rc");
    std::fs::write(&out, rc).unwrap();
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_required()
        .unwrap();
}
