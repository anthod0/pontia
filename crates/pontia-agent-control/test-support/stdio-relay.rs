// Standalone entry point used by the isolated client integration tests.
#[tokio::main]
async fn main() -> pontia_core::Result<()> {
    pontia_agent_control::ipc::relay::run_from_stdin().await
}
