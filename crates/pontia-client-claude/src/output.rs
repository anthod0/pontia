use crate::peer::{Input, Peer, field};
use pontia_application::{
    EventIngestService, LiveOutputBatch, LiveOutputIdentity, LiveOutputProducer,
    LiveOutputPublishOutcome, LiveOutputSnapshotReplacement, LiveOutputSource, LiveOutputUpdate,
    LiveToolOutcome, LiveToolPhase, LiveToolResult, LiveToolStatus,
};
use pontia_core::{Error, Result};
use serde_json::Value;

pub(crate) async fn publish(
    events: &EventIngestService,
    peer: &Peer,
    input: &mut Input,
    event: &Value,
) -> Result<()> {
    let output = events.live_output().ok_or_else(invalidated)?;
    let turn = input.turn.as_ref().ok_or_else(invalidated)?;
    let producer = LiveOutputProducer {
        identity: LiveOutputIdentity {
            session_id: peer.session.clone(),
            turn_id: turn.clone(),
            stream_id: format!("{}:{}", peer.runtime, input.id),
        },
        source: LiveOutputSource::RuntimeBound {
            runtime_id: peer.runtime.clone(),
        },
    };
    if input.sequence == 0 {
        accept(
            input,
            output
                .replace_snapshot(LiveOutputSnapshotReplacement {
                    producer: producer.clone(),
                    sequence: 1,
                    items: vec![],
                })
                .await?,
        )?;
    }
    let snapshot = output
        .snapshot(&peer.session, input.turn.as_ref().ok_or_else(invalidated)?)
        .ok_or_else(invalidated)?;
    if snapshot.identity != producer.identity || snapshot.sequence != input.sequence {
        return Err(invalidated());
    }
    let updates = match event["kind"].as_str() {
        Some("text") => vec![LiveOutputUpdate::AssistantTextDelta {
            item_id: format!("text:{}", field(event, "item_id")?),
            delta: field(event, "delta")?.into(),
        }],
        Some("tool") => vec![LiveOutputUpdate::ToolCall {
            item_id: format!("tool:{}", field(event, "call_id")?),
            call_id: field(event, "call_id")?.into(),
            tool_name: field(event, "name")?.into(),
            arguments: event["arguments"].clone(),
            managed_tool_use: None,
        }],
        Some("tool_result") => {
            let failed = event["failed"].as_bool().ok_or_else(invalidated)?;
            vec![
                LiveOutputUpdate::ToolStatusUpdated {
                    call_id: field(event, "call_id")?.into(),
                    status: LiveToolStatus {
                        phase: LiveToolPhase::Ended,
                        outcome: Some(if failed {
                            LiveToolOutcome::Failed
                        } else {
                            LiveToolOutcome::Succeeded
                        }),
                    },
                },
                LiveOutputUpdate::ToolResult {
                    call_id: field(event, "call_id")?.into(),
                    result: LiveToolResult {
                        content: (!failed).then(|| event["content"].clone()),
                        error: failed.then(|| event["content"].clone()),
                        truncated: false,
                    },
                },
            ]
        }
        _ => return Err(invalidated()),
    };
    let updates = updates
        .into_iter()
        .map(|update| update.bound_tool_content(&snapshot.items))
        .collect();
    accept(
        input,
        output
            .publish_batch(LiveOutputBatch {
                producer,
                first_sequence: input.sequence + 1,
                updates,
            })
            .await?,
    )
}
fn accept(input: &mut Input, outcome: LiveOutputPublishOutcome) -> Result<()> {
    match outcome {
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence, ..
        } => {
            input.sequence = accepted_sequence;
            Ok(())
        }
        _ => Err(invalidated()),
    }
}
fn invalidated() -> Error {
    Error::CapabilityUnavailable("Claude Agent live output is unavailable".into())
}
