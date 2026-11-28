use crate::peer::{Input, Peer, field};
use pontia_agent_control::{
    EventIngestService, LiveOutputBatch, LiveOutputIdentity, LiveOutputProducer,
    LiveOutputPublishOutcome, LiveOutputSnapshotReplacement, LiveOutputSource, LiveOutputUpdate,
    LiveReasoningBound, LiveToolOutcome, LiveToolPhase, LiveToolResult, LiveToolStatus,
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
        Some("reasoning") => vec![LiveOutputUpdate::ReasoningDelta {
            item_id: format!("reasoning:{}", field(event, "item_id")?),
            delta: field(event, "delta")?.into(),
            truncated: false,
        }],
        Some("input") => vec![LiveOutputUpdate::UserInput {
            item_id: format!("input:{}", field(event, "item_id")?),
            text: field(event, "text")?.into(),
        }],
        Some("tool") => {
            let name = field(event, "name")?;
            vec![LiveOutputUpdate::ToolCall {
                item_id: format!("tool:{}", field(event, "call_id")?),
                call_id: field(event, "call_id")?.into(),
                tool_name: name.into(),
                arguments: event["arguments"].clone(),
                managed_tool_use: crate::tool_use::from_call(name, &event["arguments"]),
            }]
        }
        Some("tool_progress") => vec![LiveOutputUpdate::ToolStatusUpdated {
            call_id: field(event, "call_id")?.into(),
            status: LiveToolStatus {
                phase: LiveToolPhase::Running,
                outcome: None,
            },
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
    let mut bounded = Vec::with_capacity(updates.len());
    for update in updates {
        match update.bound_reasoning(&snapshot.items) {
            LiveReasoningBound::Update(update) => {
                bounded.push(update.bound_tool_content(&snapshot.items));
            }
            LiveReasoningBound::Snapshot(items) => {
                accept(
                    input,
                    output
                        .replace_snapshot(LiveOutputSnapshotReplacement {
                            producer,
                            sequence: input.sequence + 1,
                            items,
                        })
                        .await?,
                )?;
                return Ok(());
            }
            LiveReasoningBound::Drop => return Ok(()),
        }
    }
    accept(
        input,
        output
            .publish_batch(LiveOutputBatch {
                producer,
                first_sequence: input.sequence + 1,
                updates: bounded,
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
    Error::Unavailable("Claude Code live output is unavailable".into())
}
