use pontia_runtime::local_service::{absolute_utf8_path as utf8_path, systemd_quote};
use std::path::{Path, PathBuf};

pub const SYSTEMD_SERVICE_NAME: &str = "pontia.service";
pub const LAUNCHD_LABEL: &str = "dev.pontia.pontiad";

pub fn render_systemd(
    pontiad: &Path,
    pontia_home: &Path,
    auth_origin: &str,
) -> Result<String, String> {
    render_systemd_with_environment(pontiad, pontia_home, auth_origin, &[])
}

pub fn render_systemd_with_environment(
    pontiad: &Path,
    pontia_home: &Path,
    auth_origin: &str,
    environment_paths: &[(String, PathBuf)],
) -> Result<String, String> {
    let pontiad = utf8_path(pontiad, "pontiad executable")?;
    let pontia_home = utf8_path(pontia_home, "PONTIA_HOME")?;
    let auth_origin = systemd_quote(auth_origin);
    let mut environment = String::new();
    for (name, path) in environment_paths {
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err("invalid service environment variable name".into());
        }
        let path = utf8_path(path, name)?;
        environment.push_str(&format!("Environment=\"{name}={}\"\n", systemd_quote(path)));
    }
    Ok(format!(
        "[Unit]\nDescription=Pontia Control Plane\nAfter=network.target\n\n[Service]\nType=simple\nExecStart=\"{}\"\nEnvironment=\"PONTIA_HOME={}\"\nEnvironment=\"PONTIA_AUTH_ORIGIN={}\"\n{}Restart=on-failure\n\n[Install]\nWantedBy=default.target\n",
        systemd_quote(pontiad),
        systemd_quote(pontia_home),
        auth_origin,
        environment,
    ))
}

pub fn render_launchd(
    pontiad: &Path,
    pontia_home: &Path,
    auth_origin: &str,
) -> Result<String, String> {
    let pontiad = xml_escape(utf8_path(pontiad, "pontiad executable")?)?;
    let pontia_home = xml_escape(utf8_path(pontia_home, "PONTIA_HOME")?)?;
    let auth_origin = xml_escape(auth_origin)?;
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
    <key>PONTIA_AUTH_ORIGIN</key>
    <string>{auth_origin}</string>
  </dict>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#
    ))
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
