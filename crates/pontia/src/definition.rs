use std::path::Path;

pub const SYSTEMD_SERVICE_NAME: &str = "pontia.service";
pub const CODEX_SYSTEMD_SERVICE_NAME: &str = "pontia-codex.service";
pub const LAUNCHD_LABEL: &str = "dev.pontia.pontiad";

pub fn render_systemd(pontiad: &Path, pontia_home: &Path) -> Result<String, String> {
    render_systemd_with_codex(pontiad, pontia_home, None)
}

pub fn render_systemd_with_codex(
    pontiad: &Path,
    pontia_home: &Path,
    codex_home: Option<&Path>,
) -> Result<String, String> {
    let pontiad = utf8_path(pontiad, "pontiad executable")?;
    let pontia_home = utf8_path(pontia_home, "PONTIA_HOME")?;
    let codex_environment = codex_home
        .map(|path| {
            utf8_path(path, "CODEX_HOME")
                .map(|path| format!("Environment=\"CODEX_HOME={}\"\n", systemd_quote(path)))
        })
        .transpose()?
        .unwrap_or_default();
    Ok(format!(
        "[Unit]\nDescription=Pontia Control Plane\nAfter=network.target\n\n[Service]\nType=simple\nExecStart=\"{}\"\nEnvironment=\"PONTIA_HOME={}\"\n{}Restart=on-failure\n\n[Install]\nWantedBy=default.target\n",
        systemd_quote(pontiad),
        systemd_quote(pontia_home),
        codex_environment,
    ))
}

pub fn render_codex_systemd(codex: &Path, codex_home: &Path) -> Result<String, String> {
    let codex = utf8_path(codex, "Codex executable")?;
    let codex_home = utf8_path(codex_home, "CODEX_HOME")?;
    Ok(format!(
        "[Unit]\nDescription=Codex App Server Daemon Startup\nAfter=network.target\n\n[Service]\nType=oneshot\nExecStart=\"{}\" app-server daemon start\nEnvironment=\"CODEX_HOME={}\"\n\n[Install]\nWantedBy=default.target\n",
        systemd_quote(codex),
        systemd_quote(codex_home),
    ))
}

pub fn render_launchd(pontiad: &Path, pontia_home: &Path) -> Result<String, String> {
    let pontiad = xml_escape(utf8_path(pontiad, "pontiad executable")?)?;
    let pontia_home = xml_escape(utf8_path(pontia_home, "PONTIA_HOME")?)?;
    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{LAUNCHD_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{pontiad}</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PONTIA_HOME</key>
    <string>{pontia_home}</string>
  </dict>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#
    ))
}

fn utf8_path<'a>(path: &'a Path, description: &str) -> Result<&'a str, String> {
    if !path.is_absolute() {
        return Err(format!(
            "{description} must be an absolute path: {}",
            path.display()
        ));
    }
    path.to_str()
        .ok_or_else(|| format!("{description} is not valid UTF-8: {}", path.display()))
}

fn systemd_quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '%' => escaped.push_str("%%"),
            character => escaped.push(character),
        }
    }
    escaped
}

fn xml_escape(value: &str) -> Result<String, String> {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if character.is_control() && !matches!(character, '\t' | '\n' | '\r') {
            return Err("path contains a character that XML 1.0 cannot represent".to_string());
        }
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            character => escaped.push(character),
        }
    }
    Ok(escaped)
}
