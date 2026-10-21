use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/hayai.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    assert_eq!(
        env::var("CARGO_CFG_TARGET_ENV").as_deref(),
        Ok("msvc"),
        "Windows icon resources require the MSVC toolchain"
    );
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let rc_file = out.join("icon.rc");
    let resource = out.join("icon.res");
    let icon = root
        .join("assets/hayai.ico")
        .to_string_lossy()
        .replace('\\', "/");
    fs::write(&rc_file, format!("1 ICON \"{icon}\"\n")).unwrap();
    let sdk = PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK is required"))
        .join("Windows Kits/10/bin");
    let mut versions: Vec<_> = fs::read_dir(sdk)
        .expect("Install the Windows 10/11 SDK")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    versions.sort();
    let rc = versions
        .iter()
        .rev()
        .map(|v| v.join("x64/rc.exe"))
        .find(|p| p.is_file())
        .expect("Windows SDK rc.exe is required");
    let status = Command::new(rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&resource)
        .arg(&rc_file)
        .status()
        .expect("run rc.exe");
    assert!(status.success(), "icon resource compilation failed");
    println!("cargo:rustc-link-arg-bins={}", resource.display());
}
