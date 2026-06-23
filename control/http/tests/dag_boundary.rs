#[test]
fn http_can_depend_on_dag_feature_crate_boundary() {
    let _ = std::any::type_name::<pontia_dag::DagService>();
    let _ = std::any::type_name::<pontia_dag::profiles::AgentProfileService>();
}
