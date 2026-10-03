fn main() {
    println!("cargo:rerun-if-env-changed=RELEASE_PUBLIC_KEY");
    if let Ok(key) = std::env::var("RELEASE_PUBLIC_KEY") {
        assert!(
            !key.trim().is_empty(),
            "RELEASE_PUBLIC_KEY must not be empty"
        );
        println!(
            "cargo:rustc-env=PONTIA_RELEASE_PUBLIC_KEY={}",
            key.replace('\n', "\\n")
        );
    }
}
