#![allow(dead_code)]
pub fn clients() -> pontia_agent_control::clients::ClientRegistry {
    let mut clients = pontia_agent_control::clients::ClientRegistry::default();
    clients.register(pontia_agent_control::client_contract::test_registration());
    clients.register(pontia_client_pi::registration(None));
    clients.register(pontia_client_codex::registration());
    clients
}
