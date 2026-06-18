use std::path::{Path, PathBuf};

pub(super) fn expand_home_prefix(path: &Path) -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) => expand_home_prefix_with_home(path, Path::new(&home)),
        None => path.to_path_buf(),
    }
}

pub(super) fn expand_home_prefix_with_home(path: &Path, home: &Path) -> PathBuf {
    if path == Path::new("~") {
        return home.to_path_buf();
    }

    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn expands_leading_tilde_before_opening_lbug_database() {
        let expanded = super::expand_home_prefix_with_home(
            Path::new("~/.local/share/pontia/graph/lbug"),
            Path::new("/home/example"),
        );

        assert_eq!(
            expanded,
            Path::new("/home/example/.local/share/pontia/graph/lbug")
        );
    }
}
