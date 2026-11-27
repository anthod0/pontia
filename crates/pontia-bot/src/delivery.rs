use pontia_agent_control::{AgentControl, DeliveryError, InputPayload, SendMode, SendOutcome};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BotInputDelivery {
    Accepted,
    Queued,
}

impl From<SendOutcome> for BotInputDelivery {
    fn from(outcome: SendOutcome) -> Self {
        match outcome {
            SendOutcome::Delivered => Self::Accepted,
            SendOutcome::Queued { .. } => Self::Queued,
        }
    }
}

pub async fn send_input(
    control: &AgentControl,
    session_id: &str,
    input: InputPayload,
    mode: SendMode,
) -> Result<SendOutcome, DeliveryError> {
    let result = control
        .inbox_commands()
        .send(session_id, input.clone(), mode)
        .await;
    match result {
        Err(DeliveryError::Unsupported(_)) if mode == SendMode::Immediate => {
            control
                .inbox_commands()
                .send(session_id, input, SendMode::Queue)
                .await
        }
        result => result,
    }
}
