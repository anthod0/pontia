use super::*;
use pontia_agent_clients::get_client_spec;

pub(super) fn client_readiness_mode(client_type: &str) -> Result<ReadinessMode> {
    get_client_spec(client_type)
        .map(|spec| spec.adapter.readiness)
        .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))
}
