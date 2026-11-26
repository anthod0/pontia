use pontia_agent_control::client_contract::{DetectionContext, install::InstallProvenance};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::Path,
};

fn script(path: &Path, body: &str) {
    fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn registered_installers_check_each_method_and_recognize_managed_links() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    let home = root.path().join("pontia");
    fs::create_dir(&bin).unwrap();
    fs::create_dir_all(home.join("tools/bin")).unwrap();
    let vars = HashMap::from([
        ("PONTIA_HOME".into(), home.display().to_string()),
        ("PATH".into(), bin.display().to_string()),
    ]);
    let registry = pontia_clients::registration(&pontia_clients::config_from_vars(&vars).unwrap());
    let context = DetectionContext {
        vars: &vars,
        pontia_home: &home,
    };
    script(&bin.join("codex"), "# Pontia Codex launcher\nexit 1");
    symlink("/bin/true", bin.join("bun")).unwrap();
    for client in ["pi", "codex"] {
        let installer = registry.installer(client, "default").unwrap();
        for (version, ready) in [
            ("16.0.0", false),
            ("22.18.0", client == "codex"),
            ("22.19.0-rc.1", client == "codex"),
            ("22.19.0", true),
        ] {
            script(&bin.join("node"), &format!("echo v{version}"));
            let spec = installer.inspect(&context);
            assert_eq!(spec.provenance, InstallProvenance::NotInstalled);
            let method = |id| spec.methods.iter().find(|m| m.id == id).unwrap();
            assert_eq!(method("bun_managed").ready(), ready);
            assert!(!method("npm_managed").ready());
        }
        let prefix = home.join("tools").join(client).join("node_modules/.bin");
        fs::create_dir_all(&prefix).unwrap();
        script(&prefix.join(client), "exit 0");
        symlink(prefix.join(client), home.join("tools/bin").join(client)).unwrap();
        assert_eq!(
            installer.inspect(&context).provenance,
            InstallProvenance::PontiaManaged
        );
    }
}
