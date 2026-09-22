use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::StatusCode,
};
use pontia_application::{
    AppState, LiveOutputBatch, LiveOutputClose, LiveOutputIdentity, LiveOutputItem,
    LiveOutputProducer, LiveOutputPublishOutcome, LiveOutputSnapshotReplacement, LiveOutputUpdate,
};
use serde::{Deserialize, Serialize};

use super::response::ApiError;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InternalLiveOutputRequest {
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
pub struct InternalLiveOutputResponse {
    accepted: bool,
    duplicate: bool,
    resync_required: bool,
    accepted_sequence: u64,
}

pub async fn post_live_output(
    State(state): State<AppState>,
    request: Result<Json<InternalLiveOutputRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<InternalLiveOutputResponse>), ApiError> {
    let Json(request) = request.map_err(|err| ApiError::invalid_request(err.body_text()))?;
    let service = state.live_output();
    let outcome = match request {
        InternalLiveOutputRequest::Append {
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
        InternalLiveOutputRequest::Snapshot {
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
        InternalLiveOutputRequest::StreamClosed {
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

    Ok(response_from_outcome(outcome))
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

fn response_from_outcome(
    outcome: LiveOutputPublishOutcome,
) -> (StatusCode, Json<InternalLiveOutputResponse>) {
    match outcome {
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence,
            duplicate,
        } => (
            StatusCode::OK,
            Json(InternalLiveOutputResponse {
                accepted: true,
                duplicate,
                resync_required: false,
                accepted_sequence,
            }),
        ),
        LiveOutputPublishOutcome::SnapshotRequired { accepted_sequence } => (
            StatusCode::CONFLICT,
            Json(InternalLiveOutputResponse {
                accepted: false,
                duplicate: false,
                resync_required: true,
                accepted_sequence,
            }),
        ),
    }
}
