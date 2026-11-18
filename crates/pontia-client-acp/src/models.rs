use crate::peer::SessionState;
use agent_client_protocol::v1::schema::{
    SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
};
use pontia_agent_control::EventIngestService;
use pontia_core::{Result, domain::EventType};
use serde_json::json;

pub(crate) async fn replace(
    state: &mut SessionState,
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    options: Vec<SessionConfigOption>,
) -> Result<()> {
    let current = options.into_iter().find_map(|option| {
        if option.category != Some(SessionConfigOptionCategory::Model) {
            return None;
        }
        let SessionConfigKind::Select(select) = option.kind else {
            return None;
        };
        Some(select.current_value.to_string())
    });
    if let Some(model) = &current
        && state.current_model.as_ref() != Some(model)
    {
        events
            .report_client_fact(
                session,
                Some(runtime),
                EventType::SessionModelUpdated,
                json!({"model":model}),
            )
            .await?;
    }
    state.current_model = current;
    Ok(())
}
