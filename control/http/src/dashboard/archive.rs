use std::{
    io::Cursor,
    path::{Component, Path, PathBuf},
};

use flate2::read::GzDecoder;

pub(super) fn extract(source: &str, bytes: &[u8], destination: &Path) -> Result<(), String> {
    let source_path = source.split(['?', '#']).next().unwrap_or(source);
    if source_path.ends_with(".zip") {
        extract_zip(bytes, destination)
    } else if source_path.ends_with(".tar.gz") || source_path.ends_with(".tgz") {
        extract_targz(bytes, destination)
    } else {
        Err("remote dashboard source must end with .zip, .tar.gz, or .tgz".to_string())
    }
}

fn extract_zip(bytes: &[u8], destination: &Path) -> Result<(), String> {
    let reader = Cursor::new(bytes);
    let mut archive =
        zip::ZipArchive::new(reader).map_err(|err| format!("invalid zip archive: {err}"))?;
    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|err| format!("failed to read zip entry: {err}"))?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| format!("unsafe zip entry path: {}", file.name()))?;
        let output = destination.join(enclosed);
        if file.is_dir() {
            std::fs::create_dir_all(&output)
                .map_err(|err| format!("failed to create zip directory: {err}"))?;
        } else {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| format!("failed to create zip parent: {err}"))?;
            }
            let mut output_file = std::fs::File::create(&output)
                .map_err(|err| format!("failed to create zip output: {err}"))?;
            std::io::copy(&mut file, &mut output_file)
                .map_err(|err| format!("failed to write zip output: {err}"))?;
        }
    }
    Ok(())
}

fn extract_targz(bytes: &[u8], destination: &Path) -> Result<(), String> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|err| format!("invalid tar.gz archive: {err}"))?;
    for entry in entries {
        let mut entry = entry.map_err(|err| format!("failed to read tar entry: {err}"))?;
        let path = entry
            .path()
            .map_err(|err| format!("failed to read tar path: {err}"))?;
        let entry_type = entry.header().entry_type();
        if !(entry_type.is_file() || entry_type.is_dir()) {
            return Err(format!("unsupported tar entry type for {}", path.display()));
        }
        let safe_path = safe_relative_path(&path)?;
        entry
            .unpack(destination.join(safe_path))
            .map_err(|err| format!("failed to unpack tar entry: {err}"))?;
    }
    Ok(())
}

fn safe_relative_path(path: &Path) -> Result<PathBuf, String> {
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return Err(format!("unsafe archive entry path: {}", path.display())),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err("empty archive entry path".to_string());
    }
    Ok(clean)
}
