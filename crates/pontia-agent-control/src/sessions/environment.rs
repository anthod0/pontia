use crate::client_contract::ClientOperation;
use std::collections::BTreeMap;
/// Restores caller-owned launch context for an existing Session.
pub trait SessionEnvironment: Send + Sync {
    fn environment<'a>(
        &'a self,
        session_id: &'a str,
    ) -> ClientOperation<'a, BTreeMap<String, String>>;
}
