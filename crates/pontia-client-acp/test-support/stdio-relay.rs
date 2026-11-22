// Standalone entry point used by the isolated client integration tests.
#[tokio::main]
async fn main() -> pontia_core::Result<()> {
    pontia_client_acp::relay::run_from_stdin().await
}
