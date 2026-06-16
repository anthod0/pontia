#[test]
fn lbug_feature_is_enabled_by_default() {
    const {
        assert!(
            cfg!(feature = "lbug"),
            "pontia's default build must include the mandatory lbug feature"
        );
    }
}
