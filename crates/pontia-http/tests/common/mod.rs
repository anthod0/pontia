#![allow(dead_code)]

pub(crate) mod generic_client;
pub(crate) mod test_app;

pub(crate) mod reporting;

pub(crate) mod clients;

pub(crate) async fn wait_for_turn_id(
    state: &pontia_application::AppState,
    session_id: &str,
) -> String {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(turn) = state
                .control()
                .queries()
                .list_turns(session_id)
                .await
                .expect("list turns")
                .last()
            {
                return turn.turn_id.clone();
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("turn dispatch timeout")
}
