use super::*;
use crate::agent_clients::get_client_definition;

pub(super) fn client_readiness_mode(client_type: &str) -> Result<ReadinessMode> {
    get_client_definition(client_type)
        .map(|spec| spec.backend.readiness)
        .ok_or_else(|| Error::Domain(format!("unsupported client_type: {client_type}")))
}
