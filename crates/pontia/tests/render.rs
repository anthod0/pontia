use std::path::Path;

use pontia::definition::{
    render_launchd, render_launchd_with_environment, render_systemd_with_environment,
};

const AUTH_ORIGIN: &str = "https://dev.pontia.example";

#[test]
fn renders_systemd_user_service() {
    let rendered = render_systemd_with_environment(
        Path::new("/opt/Pontia App/bin/pontiad"),
        Path::new("/home/alice/Pontia Home%dev"),
        AUTH_ORIGIN,
        &[],
    )
    .expect("valid paths");

    assert_eq!(
        rendered,
        "[Unit]\nDescription=Pontia Control Plane\nAfter=network.target\n\n[Service]\nType=simple\nExecStart=\"/opt/Pontia App/bin/pontiad\"\nEnvironment=\"PONTIA_HOME=/home/alice/Pontia Home%%dev\"\nEnvironment=\"PONTIA_AUTH_ORIGIN=https://dev.pontia.example\"\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n"
    );
}

#[test]
fn systemd_rendering_escapes_unit_syntax() {
    let rendered = render_systemd_with_environment(
        Path::new("/opt/pontia\\build/\"pontiad\""),
        Path::new("/home/alice/pontia\\\"home"),
        AUTH_ORIGIN,
        &[("CODEX_HOME".into(), "/home/alice/codex%home".into())],
    )
    .expect("valid paths");

    assert!(rendered.contains("ExecStart=\"/opt/pontia\\\\build/\\\"pontiad\\\"\""));
    assert!(rendered.contains("Environment=\"PONTIA_HOME=/home/alice/pontia\\\\\\\"home\""));
    assert!(rendered.contains("Environment=\"CODEX_HOME=/home/alice/codex%%home\""));
}

#[test]
fn renders_launch_agent_plist_with_xml_escaping() {
    let rendered = render_launchd(
        Path::new("/Applications/Pontia & Co/pontiad"),
        Path::new("/Users/alice/Pontia <dev>"),
        AUTH_ORIGIN,
    )
    .expect("valid paths");

    assert_eq!(
        rendered,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>dev.pontia.pontiad</string>
  <key>ProgramArguments</key>
  <array>
    <string>/Applications/Pontia &amp; Co/pontiad</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>PONTIA_HOME</key>
    <string>/Users/alice/Pontia &lt;dev&gt;</string>
    <key>PONTIA_AUTH_ORIGIN</key>
    <string>https://dev.pontia.example</string>
  </dict>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#
    );
}

#[test]
fn launchd_rendering_preserves_client_paths_and_executable_search_path() {
    let rendered = render_launchd_with_environment(
        Path::new("/Applications/Pontia App/pontiad"),
        Path::new("/Users/alice/Pontia Home"),
        AUTH_ORIGIN,
        &[("CODEX_HOME".into(), "/Users/alice/Codex & Data".into())],
        &["/opt/homebrew/bin".into(), "/Users/alice/.local/bin".into()],
    )
    .expect("valid launchd environment");

    assert!(
        rendered
            .contains("<key>CODEX_HOME</key>\n    <string>/Users/alice/Codex &amp; Data</string>")
    );
    assert!(rendered.contains(
        "<key>PATH</key>\n    <string>/opt/homebrew/bin:/Users/alice/.local/bin</string>"
    ));
}

#[test]
fn renderers_reject_relative_paths() {
    assert!(
        render_systemd_with_environment(
            Path::new("bin/pontiad"),
            Path::new("/home/alice"),
            AUTH_ORIGIN,
            &[],
        )
        .is_err()
    );
    assert!(
        render_launchd(
            Path::new("/usr/bin/pontiad"),
            Path::new("pontia"),
            AUTH_ORIGIN,
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn renderers_reject_non_utf8_paths() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let path = Path::new(OsStr::from_bytes(b"/home/alice/\xff"));
    assert!(
        render_systemd_with_environment(Path::new("/usr/bin/pontiad"), path, AUTH_ORIGIN, &[])
            .is_err()
    );
    assert!(render_launchd(Path::new("/usr/bin/pontiad"), path, AUTH_ORIGIN).is_err());
}
