use crate::AcpLaunch;
use pontia_agent_control::client_contract::LaunchFile;
use std::{fs, io, os::unix::fs::symlink, path::Path};

pub(crate) const FILES: &[LaunchFile] = &[LaunchFile {
    path: ".agents/agents/pontia-restricted.md",
    contents: "---
name: pontia-restricted
description: Pontia restricted Session
tools: [read_file, run_command]
inheritMcp: false
inheritCustomizations: false
mainAgent: true
subagent: false
---
",
}];

pub(crate) fn configure_launch(home: &Path, launch: &mut AcpLaunch) -> io::Result<()> {
    let data = home.join("clients/antigravity");
    fs::create_dir_all(&data)?;
    if let Some(user_home) = std::env::home_dir() {
        link_default_token(&data, &user_home)?;
    }
    launch.args.extend([
        "--gemini-dir".into(),
        data.to_string_lossy().into_owned(),
        "--agent".into(),
        "pontia-restricted".into(),
        "--include-default-customizations=false".into(),
        "--dangerously-skip-permissions".into(),
    ]);
    Ok(())
}

fn link_default_token(data: &Path, user_home: &Path) -> io::Result<()> {
    const TOKEN: &str = "antigravity-cli/antigravity-oauth-token";
    let token = data.join(TOKEN);
    if token.try_exists()? {
        return Ok(());
    }
    let source = user_home.join(".gemini").join(TOKEN);
    match fs::metadata(&source) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    }

    fs::create_dir_all(token.parent().unwrap())?;
    // agy 1.5.15 refreshes this file in place, preserving the link. Concurrent
    // refreshes by the TUI and restricted sessions have no cross-process lock.
    match symlink(source, token) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_only_the_default_token() {
        let root = tempfile::tempdir().unwrap();
        let cli = root.path().join(".gemini/antigravity-cli");
        fs::create_dir_all(&cli).unwrap();
        let source = cli.join("antigravity-oauth-token");
        fs::write(&source, "token").unwrap();
        fs::write(cli.join("settings.json"), "{}").unwrap();
        let data = root.path().join("restricted");

        link_default_token(&data, root.path()).unwrap();

        let restricted_cli = data.join("antigravity-cli");
        assert!(!data.is_symlink());
        assert!(!restricted_cli.is_symlink());
        assert_eq!(
            fs::read_link(restricted_cli.join("antigravity-oauth-token")).unwrap(),
            source
        );
        assert!(!restricted_cli.join("settings.json").exists());
    }

    #[test]
    fn preserves_independent_credentials_and_existing_links() {
        let root = tempfile::tempdir().unwrap();
        let cli = root.path().join(".gemini/antigravity-cli");
        fs::create_dir_all(&cli).unwrap();
        fs::write(cli.join("antigravity-oauth-token"), "default").unwrap();
        let other = root.path().join("other-token");
        fs::write(&other, "other").unwrap();

        for (directory, linked) in [("independent", false), ("linked", true)] {
            let data = root.path().join(directory);
            let restricted_cli = data.join("antigravity-cli");
            fs::create_dir_all(&restricted_cli).unwrap();
            let token = restricted_cli.join("antigravity-oauth-token");
            if linked {
                symlink(&other, &token).unwrap();
            } else {
                fs::write(&token, "other").unwrap();
            }

            link_default_token(&data, root.path()).unwrap();

            assert_eq!(token.is_symlink(), linked);
            assert_eq!(fs::read_to_string(token).unwrap(), "other");
        }
    }

    #[test]
    fn skips_a_missing_or_non_file_default_token() {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("restricted");
        let token = data.join("antigravity-cli/antigravity-oauth-token");

        link_default_token(&data, root.path()).unwrap();
        assert!(!token.is_symlink());

        fs::create_dir_all(
            root.path()
                .join(".gemini/antigravity-cli/antigravity-oauth-token"),
        )
        .unwrap();
        link_default_token(&data, root.path()).unwrap();
        assert!(!token.is_symlink());
    }
}
