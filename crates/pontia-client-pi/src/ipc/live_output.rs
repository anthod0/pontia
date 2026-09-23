use pontia_application::{
    AppState, LiveOutputBatch, LiveOutputClose, LiveOutputIdentity, LiveOutputItem,
    LiveOutputProducer, LiveOutputPublishOutcome, LiveOutputSnapshotReplacement, LiveOutputUpdate,
};
use serde::{Deserialize, Serialize};

use super::Attach;
use pontia_core::{Error, Result};
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum LiveOutputRequest {
    Append {
        session_id: String,
        turn_id: String,
        runtime_instance_id: String,
        stream_id: String,
        first_sequence: u64,
        updates: Vec<LiveOutputUpdate>,
    },
    Snapshot {
        session_id: String,
        turn_id: String,
        runtime_instance_id: String,
        stream_id: String,
        sequence: u64,
        items: Vec<LiveOutputItem>,
    },
    StreamClosed {
        session_id: String,
        turn_id: String,
        runtime_instance_id: String,
        stream_id: String,
        sequence: u64,
    },
}

#[derive(Debug, Serialize)]
struct LiveOutputResponse {
    accepted: bool,
    duplicate: bool,
    resync_required: bool,
    accepted_sequence: u64,
}

pub(super) async fn publish(state: &AppState, identity: &Attach, params: Value) -> Result<Value> {
    let request: LiveOutputRequest = serde_json::from_value(params)?;
    let (session_id, runtime_instance_id) = match &request {
        LiveOutputRequest::Append {
            session_id,
            runtime_instance_id,
            ..
        }
        | LiveOutputRequest::Snapshot {
            session_id,
            runtime_instance_id,
            ..
        }
        | LiveOutputRequest::StreamClosed {
            session_id,
            runtime_instance_id,
            ..
        } => (session_id, runtime_instance_id),
    };
    if session_id != &identity.session_id || runtime_instance_id != &identity.runtime_instance_id {
        return Err(Error::StateConflict(
            "Pi live output does not match its connection identity".into(),
        ));
    }
    let service = state.live_output();
    let outcome = match request {
        LiveOutputRequest::Append {
            session_id,
            turn_id,
            runtime_instance_id,
            stream_id,
            first_sequence,
            updates,
        } => {
            service
                .publish_batch(LiveOutputBatch {
                    producer: producer(session_id, turn_id, stream_id, runtime_instance_id),
                    first_sequence,
                    updates,
                })
                .await?
        }
        LiveOutputRequest::Snapshot {
            session_id,
            turn_id,
            runtime_instance_id,
            stream_id,
            sequence,
            items,
        } => {
            service
                .replace_snapshot(LiveOutputSnapshotReplacement {
                    producer: producer(session_id, turn_id, stream_id, runtime_instance_id),
                    sequence,
                    items,
                })
                .await?
        }
        LiveOutputRequest::StreamClosed {
            session_id,
            turn_id,
            runtime_instance_id,
            stream_id,
            sequence,
        } => {
            service
                .close(LiveOutputClose {
                    producer: producer(session_id, turn_id, stream_id, runtime_instance_id),
                    sequence,
                })
                .await?
        }
    };

    Ok(serde_json::to_value(response_from_outcome(outcome))?)
}

fn producer(
    session_id: String,
    turn_id: String,
    stream_id: String,
    runtime_instance_id: String,
) -> LiveOutputProducer {
    LiveOutputProducer {
        identity: LiveOutputIdentity {
            session_id,
            turn_id,
            stream_id,
        },
        runtime_instance_id,
    }
}

fn response_from_outcome(outcome: LiveOutputPublishOutcome) -> LiveOutputResponse {
    match outcome {
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence,
            duplicate,
        } => LiveOutputResponse {
            accepted: true,
            duplicate,
            resync_required: false,
            accepted_sequence,
        },
        LiveOutputPublishOutcome::SnapshotRequired { accepted_sequence } => LiveOutputResponse {
            accepted: false,
            duplicate: false,
            resync_required: true,
            accepted_sequence,
        },
    }
}
