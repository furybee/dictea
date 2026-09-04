fn main() {
    #[cfg(target_os = "macos")]
    build_foundation_models_shim();

    tauri_build::build()
}

/// Compile the Swift bridge to FoundationModels into a static lib and link it.
///
/// FoundationModels is weak-linked on purpose: the app supports macOS 10.15 and
/// the framework only exists from macOS 26, so it must resolve to null at load
/// time on older systems rather than abort the process.
#[cfg(target_os = "macos")]
fn build_foundation_models_shim() {
    use std::path::PathBuf;
    use std::process::Command;

    let source = "swift/FoundationModelsFFI.swift";
    println!("cargo:rerun-if-changed={}", source);

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let object = out_dir.join("foundation_models_ffi.o");
    let archive = out_dir.join("libdicteafm.a");

    let target = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x86_64-apple-macosx10.15",
        _ => "arm64-apple-macosx11.0",
    };

    let status = Command::new("swiftc")
        .args(["-emit-object", "-O", "-parse-as-library", "-target", target])
        .arg("-o")
        .arg(&object)
        .arg(source)
        .status()
        .expect("swiftc not found — Xcode command line tools are required");
    assert!(status.success(), "failed to compile {}", source);

    let status = Command::new("ar")
        .arg("rcs")
        .arg(&archive)
        .arg(&object)
        .status()
        .expect("ar not found");
    assert!(status.success(), "failed to archive the Swift shim");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=dicteafm");

    // Swift runtime: shipped by macOS itself since the ABI stabilised, so
    // nothing has to be bundled with the app.
    println!("cargo:rustc-link-search=native=/usr/lib/swift");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    // Back-deployment shims (libswiftCompatibility*.a). Targeting a macOS older
    // than the toolchain pulls these in, and they only live in the toolchain —
    // in two different places depending on whether the machine has the Command
    // Line Tools or a full Xcode, which is exactly the local/CI split here.
    let developer_dir = Command::new("xcode-select")
        .arg("-p")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .expect("xcode-select -p failed — Xcode command line tools are required");

    let candidates = [
        format!("{}/usr/lib/swift/macosx", developer_dir),
        format!(
            "{}/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/macosx",
            developer_dir
        ),
    ];
    let found = candidates
        .iter()
        .filter(|dir| PathBuf::from(dir).join("libswiftCompatibility56.a").is_file())
        .inspect(|dir| println!("cargo:rustc-link-search=native={}", dir))
        .count();
    assert!(
        found > 0,
        "Swift compatibility libraries not found under {} — checked {:?}",
        developer_dir,
        candidates
    );

    println!("cargo:rustc-link-arg=-Wl,-weak_framework,FoundationModels");
}
