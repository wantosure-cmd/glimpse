fn main() {
    for file in ["assets/brand/glimpse.ico", "assets/brand/glimpse.rc"] {
        println!("cargo:rerun-if-changed={file}");
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("glimpse.res");
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let parts: Vec<_> = version.split('.').collect();
    let numbers = format!("{},{},{},0", parts[0], parts[1], parts[2]);
    let rc_source = std::fs::read_to_string("assets/brand/glimpse.rc")
        .unwrap()
        .replace("0,1,0,0", &numbers)
        .replace("0.1.0", &version);
    let rc_file = output.with_extension("rc");
    std::fs::write(&rc_file, rc_source).unwrap();
    let mut candidates = vec![std::path::PathBuf::from("rc.exe")];
    if let Some(base) = std::env::var_os("ProgramFiles(x86)") {
        let base = std::path::PathBuf::from(base).join("Windows Kits/10/bin");
        if let Ok(entries) = std::fs::read_dir(base) {
            let mut sdk: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            sdk.sort();
            sdk.reverse();
            candidates.extend(
                sdk.into_iter()
                    .map(|p| p.join("x64/rc.exe"))
                    .filter(|p| p.is_file()),
            );
        }
    }
    for rc in candidates {
        if std::process::Command::new(rc)
            .args(["/nologo", "/fo"])
            .arg(&output)
            .arg(&rc_file)
            .status()
            .is_ok_and(|s| s.success())
        {
            println!("cargo:rustc-link-arg-bins={}", output.display());
            return;
        }
    }
    panic!("Windows SDK resource compiler rc.exe is required to embed the Glimpse icon");
}
