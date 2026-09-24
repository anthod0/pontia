use std::path::Path;

use pontia::definition::{render_codex_systemd, render_launchd, render_systemd_with_codex};

#[test]
fn renders_systemd_user_service() {
    let rendered = render_systemd_with_codex(
        Path::new("/opt/Pontia App/bin/pontiad"),
        Path::new("/home/alice/Pontia Home%dev"),
        None,
    )
    .expect("valid paths");

    assert_eq!(
        rendered,
        "[Unit]\nDescription=Pontia Control Plane\nAfter=network.target\n\n[Service]\nType=simple\nExecStart=\"/opt/Pontia App/bin/pontiad\"\nEnvironment=\"PONTIA_HOME=/home/alice/Pontia Home%%dev\"\nRestart=on-failure\n\n[Install]\nWantedBy=default.target\n"
    );
}

#[test]
fn systemd_rendering_escapes_unit_syntax() {
    let rendered = render_systemd_with_codex(
        Path::new("/opt/pontia\\build/\"pontiad\""),
        Path::new("/home/alice/pontia\\\"home"),
        Some(Path::new("/home/alice/codex%home")),
    )
    .expect("valid paths");

    assert!(rendered.contains("ExecStart=\"/opt/pontia\\\\build/\\\"pontiad\\\"\""));
    assert!(rendered.contains("Environment=\"PONTIA_HOME=/home/alice/pontia\\\\\\\"home\""));
    assert!(rendered.contains("Environment=\"CODEX_HOME=/home/alice/codex%%home\""));
}

#[test]
fn renders_codex_oneshot_user_service() {
    let rendered = render_codex_systemd(
        Path::new("/home/alice/Codex App/bin/codex"),
        Path::new("/home/alice/.codex%dev"),
    )
    .expect("valid paths");

    assert_eq!(
        rendered,
        "[Unit]\nDescription=Codex App Server Daemon Startup\nAfter=network.target\n\n[Service]\nType=oneshot\nExecStart=\"/home/alice/Codex App/bin/codex\" app-server daemon start\nEnvironment=\"CODEX_HOME=/home/alice/.codex%%dev\"\n\n[Install]\nWantedBy=default.target\n"
    );
    assert!(!rendered.contains("RemainAfterExit"));
}

#[test]
fn renders_launch_agent_plist_with_xml_escaping() {
    let rendered = render_launchd(
        Path::new("/Applications/Pontia & Co/pontiad"),
        Path::new("/Users/alice/Pontia <dev>"),
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
  </dict>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#
    );
}

#[test]
fn renderers_reject_relative_paths() {
    assert!(
        render_systemd_with_codex(Path::new("bin/pontiad"), Path::new("/home/alice"), None,)
            .is_err()
    );
    assert!(render_launchd(Path::new("/usr/bin/pontiad"), Path::new("pontia")).is_err());
}

#[cfg(unix)]
#[test]
fn renderers_reject_non_utf8_paths() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let path = Path::new(OsStr::from_bytes(b"/home/alice/\xff"));
    assert!(render_systemd_with_codex(Path::new("/usr/bin/pontiad"), path, None).is_err());
    assert!(render_launchd(Path::new("/usr/bin/pontiad"), path).is_err());
}
