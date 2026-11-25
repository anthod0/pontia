use std::{process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

/// Read a short, successful CLI response. Dropping the future also stops the child.
pub async fn output(command: &mut Command) -> Option<String> {
    bounded_output(command, 4096).await
}

pub async fn bounded_output(command: &mut Command, max_bytes: usize) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut child = command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .ok()?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()?
            .take(max_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        if bytes.len() > max_bytes || !child.wait().await.ok()?.success() {
            return None;
        }
        String::from_utf8(bytes)
            .ok()
            .map(|text| text.trim().to_owned())
    })
    .await
    .ok()
    .flatten()
}

pub async fn probe(command: &mut Command, prefix: &str) -> Option<String> {
    let text = output(command.arg("--version")).await?;
    if text.lines().count() != 1 {
        return None;
    }
    let mut fields = text.strip_prefix(prefix)?.split_whitespace();
    let value = fields.next()?;
    semver::Version::parse(value).ok()?;
    Some(value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reads_version_identifiers_from_native_cli_output() {
        for (line, prefix, expected) in [
            ("1.6.0", "", Some("1.6.0")),
            (
                "codex-cli 0.171.0-alpha.1",
                "codex-cli ",
                Some("0.171.0-alpha.1"),
            ),
            ("2.1.318 (Claude Code)", "", Some("2.1.318")),
            (
                "grok 1.0.92 (b06756fd6256) [stable]",
                "grok ",
                Some("1.0.92"),
            ),
            ("kiro-cli 3.5.0", "kiro-cli ", Some("3.5.0")),
            ("1.5.15", "", Some("1.5.15")),
            ("warning: requires 1.0.0", "", None),
            ("grok 1.2", "grok ", None),
            ("kiro-cli 2.3.4", "grok ", None),
        ] {
            let mut command = Command::new("sh");
            command.args(["-c", r#"test "$1" = --version && echo "$0""#, line]);
            assert_eq!(
                probe(&mut command, prefix).await.as_deref(),
                expected,
                "{line}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_hung_oversized_or_unsuccessful_responses() {
        for script in [
            "exec sleep 60",
            "head -c 4097 /dev/zero",
            "echo 1.0.0; exit 1",
        ] {
            assert!(
                output(Command::new("sh").args(["-c", script]))
                    .await
                    .is_none()
            );
        }
    }
}
