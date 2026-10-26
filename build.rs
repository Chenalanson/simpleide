// Generate the application ICO from the committed PNG artwork and embed it in
// the executable, so Explorer shows the `Simple` icon for the .exe itself.

include!("src/icon_codec.rs");

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon_codec.rs");
    println!("cargo:rerun-if-changed=assets/icon-word.png");
    println!("cargo:rerun-if-changed=assets/icon-mark.png");

    let word = decode_png(include_bytes!("assets/icon-word.png"))
        .expect("assets/icon-word.png must be a valid PNG");
    let mark = decode_png(include_bytes!("assets/icon-mark.png"))
        .expect("assets/icon-mark.png must be a valid PNG");
    let ico = encode_ico(&word, &mark).expect("failed to encode the application icon");
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let ico_path = out_dir.join("simple-embedded.ico");
    std::fs::write(&ico_path, ico).expect("failed to write the generated application icon");

    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon(
            ico_path
                .to_str()
                .expect("generated icon path must be valid UTF-8"),
        );
        resource
            .compile()
            .expect("failed to embed the application icon resource");

        // The GNU linker drops resource-only archive members because they
        // define no symbols. Pass the compiled object directly instead.
        let resource_object = out_dir.join("resource.o");
        if resource_object.exists() {
            println!("cargo:rustc-link-arg={}", resource_object.display());
        }
    }
}
