fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    let manifest = std::fs::canonicalize("app.manifest").expect("app.manifest");
    let mut manifest = manifest.to_string_lossy().into_owned();
    if let Some(stripped) = manifest.strip_prefix(r"\?\") {
        manifest = stripped.to_string();
    }
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{manifest}");
}
