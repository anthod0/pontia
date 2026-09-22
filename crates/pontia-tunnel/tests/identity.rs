use pontia_tunnel::{
    DeviceIdentity, RemoteClient,
    protocol::{self, Message},
};

#[test]
fn identity_survives_reload_and_signatures_bind_nonce_and_device() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("state/identity.json");
    let original = DeviceIdentity::load_or_create(&path).unwrap();
    let loaded = DeviceIdentity::load_or_create(&path).unwrap();
    assert_eq!(original.device_id(), loaded.device_id());
    assert_eq!(original.public_key(), loaded.public_key());
    let nonce = protocol::nonce().unwrap();
    let Message::Authenticate {
        device_id,
        signature,
        ..
    } = loaded.authenticate(&nonce, "fixture-key")
    else {
        panic!()
    };
    protocol::verify(&original.public_key(), device_id, &nonce, &signature).unwrap();
    assert!(
        protocol::verify(
            &original.public_key(),
            device_id,
            &protocol::nonce().unwrap(),
            &signature
        )
        .is_err()
    );
    assert!(
        protocol::verify(
            &original.public_key(),
            uuid::Uuid::new_v4(),
            &nonce,
            &signature
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[test]
fn corrupt_identity_is_not_replaced() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("identity.json");
    std::fs::write(&path, b"broken identity").unwrap();
    assert!(DeviceIdentity::load_or_create(&path).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"broken identity");
}

#[test]
fn concurrent_creation_uses_one_durable_identity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("identity.json");
    std::thread::scope(|scope| {
        let tasks: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| DeviceIdentity::load_or_create(&path).unwrap()))
            .collect();
        let identities: Vec<_> = tasks.into_iter().map(|task| task.join().unwrap()).collect();
        for identity in &identities {
            assert_eq!(identity.device_id(), identities[0].device_id());
            assert_eq!(identity.public_key(), identities[0].public_key());
        }
    });
}

#[test]
fn client_requires_wss_without_url_credentials_or_extra_routes() {
    for url in [
        "ws://localhost/tunnel",
        "wss://user:secret@example.com/tunnel",
        "wss://example.com/internal/v1/events",
        "wss://example.com/tunnel?token=secret",
        "wss://example.com/tunnel#fragment",
    ] {
        assert!(
            RemoteClient::new(
                url,
                DeviceIdentity::generate().unwrap(),
                "fixture-key".to_owned(),
                None
            )
            .is_err(),
            "{url}"
        );
    }
}

#[test]
fn client_rejects_missing_or_oversized_access_keys() {
    for key in [
        String::new(),
        " ".to_owned(),
        "k".repeat(protocol::MAX_ACCESS_KEY_BYTES + 1),
    ] {
        assert!(
            RemoteClient::new(
                "wss://edge.example/tunnel",
                DeviceIdentity::generate().unwrap(),
                key,
                None,
            )
            .is_err()
        );
    }
}
