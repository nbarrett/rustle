fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        cc::Build::new()
            .file("gen/apple/Sources/rustle-app/audio_session.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("rustle_ios_audio");
        println!("cargo:rustc-link-lib=framework=AVFoundation");
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rerun-if-changed=gen/apple/Sources/rustle-app/audio_session.m");
    }
    tauri_build::build()
}
