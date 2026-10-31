use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=PONTIA_CLAUDE_BUNDLE");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("claude-host.tar.gz");
    match env::var_os("PONTIA_CLAUDE_BUNDLE") {
        Some(path) => {
            let path = fs::canonicalize(path).expect("Claude host bundle must exist");
            println!("cargo:rerun-if-changed={}", path.display());
            fs::copy(path, output).expect("copy Claude host bundle");
        }
        None => fs::write(output, []).expect("write development bundle placeholder"),
    }
}
