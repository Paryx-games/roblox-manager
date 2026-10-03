fn main() {
    println!("cargo:rustc-check-cfg=cfg(rm_demo)");
    println!("cargo:rerun-if-env-changed=RM_DEMO");
    if std::env::var("RM_DEMO").as_deref() == Ok("1") {
        assert_eq!(
            std::env::var("PROFILE").as_deref(),
            Ok("debug"),
            "Demo mode is only available in debug builds"
        );
        println!("cargo:rustc-cfg=rm_demo");
    }
    tauri_build::build()
}
