use std::{collections::HashMap, fs};

use pontia::local_config::{dashboard_url, prepare};

#[test]
fn fresh_preparation_persists_only_a_secure_token_and_reuses_it() {
    let root = tempfile::tempdir().unwrap();
    let vars = HashMap::from([
        ("HOME".into(), root.path().display().to_string()),
        (
            "PONTIA_HOME".into(),
            root.path().join("pontia").display().to_string(),
        ),
        (
            "PONTIA_EXTERNAL_API_TOKEN".into(),
            "command-only-token".into(),
        ),
    ]);
    let config = prepare(&vars, |bytes| {
        getrandom::fill(bytes).map_err(|error| error.to_string())
    })
    .unwrap();
    let token = config.external_api_token.as_deref().unwrap();
    assert_eq!(token.len(), 43);
    assert_ne!(token, "command-only-token");
    let path = config.pontia_home.join("config.toml");
    let original = fs::read_to_string(&path).unwrap();
    let persisted: toml::Value = toml::from_str(&original).unwrap();
    assert_eq!(persisted.as_table().unwrap().len(), 1);
    assert_eq!(persisted["external_api_token"].as_str(), Some(token));
    assert_eq!(
        config.workspace_browser.roots[0].path,
        root.path().to_str().unwrap()
    );
    let again = prepare(&vars, |_| panic!("existing token must be reused")).unwrap();
    assert_eq!(again.external_api_token, config.external_api_token);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn adding_token_preserves_existing_settings_and_explicit_roots() {
    for roots in [
        "roots = []",
        "roots = [{ root_id = 'work', label = 'Work', path = '/work' }]",
    ] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config.toml");
        let original = format!(
            "# preserve comment\nbind_addr = '0.0.0.0:9090'\ncustom_setting = 'keep'\n[workspace_browser]\n{roots}\n"
        );
        fs::write(&path, &original).unwrap();
        let vars = HashMap::from([
            ("PONTIA_HOME".into(), root.path().display().to_string()),
            ("HOME".into(), root.path().display().to_string()),
        ]);
        let config = prepare(&vars, |bytes| {
            bytes.fill(7);
            Ok(())
        })
        .unwrap();
        let before: toml::Value = toml::from_str(&original).unwrap();
        let updated = fs::read_to_string(path).unwrap();
        let mut after: toml::Value = toml::from_str(&updated).unwrap();
        after.as_table_mut().unwrap().remove("external_api_token");
        assert_eq!(after, before);
        assert!(updated.contains("# preserve comment"));
        assert_eq!(
            config.workspace_browser.roots.len(),
            if roots == "roots = []" { 0 } else { 1 }
        );
        let url = url::Url::parse(&dashboard_url(&config).unwrap()).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.port(), Some(9090));
        assert_eq!(
            url.query_pairs().next().unwrap().1,
            config.external_api_token.unwrap()
        );
    }
}

#[test]
fn dashboard_url_encodes_existing_token_and_formats_ipv6() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("config.toml"),
        "bind_addr = '[::]:9091'\nexternal_api_token = 'token &?with spaces'\n",
    )
    .unwrap();
    let vars = HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]);
    let config = prepare(&vars, |_| panic!("existing token must be reused")).unwrap();
    let url = dashboard_url(&config).unwrap();
    assert_eq!(
        url,
        "http://[::1]:9091/dashboard?token=token+%26%3Fwith+spaces"
    );
}

#[test]
fn random_failure_does_not_create_configuration() {
    let root = tempfile::tempdir().unwrap();
    let vars = HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]);
    assert!(prepare(&vars, |_| Err("random source unavailable".into())).is_err());
    assert!(!root.path().join("config.toml").exists());
}

#[test]
fn invalid_configuration_does_not_leak_credentials_or_modify_file() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    let original = "external_api_token = 'do-not-leak'\ninvalid = [\n";
    fs::write(&path, original).unwrap();
    let vars = HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]);
    let error = prepare(&vars, |bytes| {
        bytes.fill(7);
        Ok(())
    })
    .unwrap_err();
    assert!(!error.contains("do-not-leak"));
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

#[test]
fn ephemeral_port_is_rejected_without_changing_configuration() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config.toml");
    let original = "bind_addr = '127.0.0.1:0'\n";
    fs::write(&path, original).unwrap();
    let vars = HashMap::from([("PONTIA_HOME".into(), root.path().display().to_string())]);
    assert!(
        prepare(&vars, |bytes| {
            bytes.fill(7);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), original);
}

#[test]
fn persistence_failure_is_returned() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("not-a-directory");
    fs::write(&path, "blocked").unwrap();
    let vars = HashMap::from([("PONTIA_HOME".into(), path.display().to_string())]);
    assert!(
        prepare(&vars, |bytes| {
            bytes.fill(7);
            Ok(())
        })
        .is_err()
    );
}
