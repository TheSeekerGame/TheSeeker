use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo::rerun-if-changed=game/macos/Info.plist");

    let target_os =
        env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "macos" {
        return;
    }

    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"),
    );
    let plist_path =
        manifest_dir.join("game").join("macos").join("Info.plist");
    let plist_path = plist_path
        .to_str()
        .expect("plist path must be valid UTF-8");

    println!(
        "cargo::rustc-link-arg-bin=theseeker_game=-Wl,-sectcreate,__TEXT,__info_plist,{plist_path}"
    );
}
