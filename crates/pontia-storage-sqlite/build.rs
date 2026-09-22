use std::{fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    emit_rerun_for_sql_files(Path::new("migrations"));
}

fn emit_rerun_for_sql_files(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            emit_rerun_for_sql_files(&path);
        } else if path.extension().and_then(|value| value.to_str()) == Some("sql") {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
}
