use pontia_edge::DeviceBindings;
use pontia_tunnel::DeviceIdentity;

#[tokio::test]
async fn bindings_persist_and_enforce_unique_account_device_and_key() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("edge.db");
    let bindings = DeviceBindings::open(&path).await.unwrap();
    let first = DeviceIdentity::generate().unwrap();
    let second = DeviceIdentity::generate().unwrap();
    bindings
        .bind("account-a", first.device_id(), first.public_key())
        .await
        .unwrap();
    assert!(
        bindings
            .bind("account-a", second.device_id(), second.public_key())
            .await
            .is_err()
    );
    assert!(
        bindings
            .bind("account-b", first.device_id(), second.public_key())
            .await
            .is_err()
    );
    assert!(
        bindings
            .bind("account-b", second.device_id(), first.public_key())
            .await
            .is_err()
    );
    assert!(
        bindings
            .bind(" ", second.device_id(), second.public_key())
            .await
            .is_err()
    );
    assert_eq!(bindings.public_key(second.device_id()).await.unwrap(), None);
    drop(bindings);
    let reopened = DeviceBindings::open(&path).await.unwrap();
    assert_eq!(
        reopened.public_key(first.device_id()).await.unwrap(),
        Some(first.public_key())
    );
}
