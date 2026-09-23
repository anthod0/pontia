#![allow(dead_code)]
pub fn clients() -> pontia_application::clients::ClientRegistry {
    let mut clients = pontia_application::clients::ClientRegistry::default();
    clients.register(pontia_application::client_contract::test_registration());
    clients.register(pontia_client_codex::registration());
    clients.register(pontia_client_pi::registration(None));
    clients
}
