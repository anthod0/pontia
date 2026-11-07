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
        if entry_type.is_dir() && path.as_ref() == Path::new(".") {
            continue;
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

#[cfg(test)]
mod tests {
    use flate2::{Compression, write::GzEncoder};
    use tar::{Builder, EntryType, Header};

    use super::extract;

    #[test]
    fn targz_rejects_invalid_root_entries_and_parent_paths() {
        for (name, kind) in [
            (".", EntryType::Regular),
            ("./", EntryType::Regular),
            ("", EntryType::Directory),
            ("../outside", EntryType::Directory),
            ("./../outside", EntryType::Directory),
            (".", EntryType::Symlink),
            (".", EntryType::Link),
        ] {
            let root = tempfile::tempdir().expect("test root");
            let destination = root.path().join("extracted");
            std::fs::create_dir(&destination).expect("destination");
            let mut archive = Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
            let mut header = Header::new_gnu();
            // Write raw names so the builder does not reject or normalize malformed paths.
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(kind);
            header.set_size(0);
            header.set_mode(0o755);
            if kind.is_symlink() || kind.is_hard_link() {
                header.set_link_name("../outside").expect("link target");
            }
            header.set_cksum();
            archive.append(&header, &[][..]).expect("archive entry");
            let bytes = archive.into_inner().expect("tar").finish().expect("gzip");

            assert!(
                extract("dashboard.tar.gz", &bytes, &destination).is_err(),
                "{name:?} ({kind:?})"
            );
            assert_eq!(
                std::fs::read_dir(&destination).expect("contents").count(),
                0
            );
            assert!(!root.path().join("outside").exists());
        }
    }
}
