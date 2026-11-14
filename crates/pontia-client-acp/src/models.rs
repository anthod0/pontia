use agent_client_protocol::schema::v1::{
    ErrorCode, SessionConfigId, SessionConfigKind, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOptions, SessionConfigValueId,
    SetSessionConfigOptionRequest,
};
use pontia_agent_control::{EventIngestService, runtime::ControlTarget, sessions::SessionModel};
use pontia_core::{Error, Result, domain::EventType};
use serde_json::json;
use tokio::sync::oneshot;

use crate::peer::{Peer, SessionState, protocol_error};

pub(crate) struct Models {
    id: SessionConfigId,
    current: String,
    pub options: Vec<SessionModel>,
}

pub(crate) async fn replace(
    state: &mut SessionState,
    events: &EventIngestService,
    session: &str,
    runtime: &str,
    options: Vec<SessionConfigOption>,
) -> Result<()> {
    let models = options.into_iter().find_map(|option| {
        if option.category != Some(SessionConfigOptionCategory::Model) {
            return None;
        }
        let SessionConfigKind::Select(select) = option.kind else {
            return None;
        };
        let values = match select.options {
            SessionConfigSelectOptions::Ungrouped(values) => values,
            SessionConfigSelectOptions::Grouped(groups) => {
                groups.into_iter().flat_map(|group| group.options).collect()
            }
            _ => return None,
        };
        Some(Models {
            id: option.id,
            current: select.current_value.to_string(),
            options: values
                .into_iter()
                .map(|value| SessionModel {
                    id: value.value.to_string(),
                    name: value.name,
                    description: value.description.unwrap_or_default(),
                })
                .collect(),
        })
    });
    if let Some(model) = &models
        && state.models.as_ref().map(|previous| &previous.current) != Some(&model.current)
    {
        events
            .report_client_fact(
                session,
                Some(runtime),
                EventType::SessionModelUpdated,
                json!({"model":model.current}),
            )
            .await?;
    }
    state.models = models;
    state.refresh_capabilities();
    Ok(())
}

impl Peer {
    pub async fn set_model(
        &self,
        events: EventIngestService,
        target: &ControlTarget,
        model: &str,
    ) -> Result<()> {
        let state = self.state.lock().await;
        target.validate(&events.db()).await?;
        if !state
            .capabilities
            .as_ref()
            .is_some_and(|caps| caps.set_model)
        {
            return Err(Error::CapabilityUnavailable(
                "ACP model control is unavailable".into(),
            ));
        }
        let models = state
            .models
            .as_ref()
            .expect("model capability requires options");
        if !models.options.iter().any(|option| option.id == model) {
            return Err(Error::Domain("The selected model is not available.".into()));
        }
        let request = self
            .connection
            .prepare_request(SetSessionConfigOptionRequest::new(
                state
                    .native
                    .clone()
                    .expect("ready session has native identity"),
                models.id.clone(),
                SessionConfigValueId::new(model.to_owned()),
            ));
        let key = serde_json::to_string(request.id())?;
        let shared = self.state.clone();
        let session = target.session_id.clone();
        let runtime = self.runtime.clone();
        let (send, receive) = oneshot::channel();
        // Apply the response in wire order, even if the requesting HTTP client disconnects.
        request
            .on_receiving_result(async move |result| {
                let mut state = shared.lock().await;
                let result = match result {
                    Ok(response) if state.current && !state.closing => {
                        replace(
                            &mut state,
                            &events,
                            &session,
                            &runtime,
                            response.config_options,
                        )
                        .await
                    }
                    Ok(_) => Err(Error::ControlUnknown(
                        "ACP model response belongs to an unavailable Session".into(),
                    )),
                    Err(error) if error.code == ErrorCode::InvalidParams => {
                        Err(Error::Domain(error.to_string()))
                    }
                    Err(error) if error.code == ErrorCode::MethodNotFound => {
                        Err(Error::CapabilityUnavailable(error.to_string()))
                    }
                    Err(error) => Err(protocol_error(error)),
                };
                let _ = send.send(result);
                Ok(())
            })
            .map_err(protocol_error)?;
        drop(state);
        self.receipts.wait(key).await?;
        receive
            .await
            .map_err(|_| Error::ControlUnknown("ACP model response was lost".into()))?
    }
}
