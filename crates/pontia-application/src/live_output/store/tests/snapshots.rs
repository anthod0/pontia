use crate::live_output::{
    LiveOutputBatch, LiveOutputItem, LiveOutputPublishOutcome, LiveOutputSnapshotReplacement,
    LiveOutputStore, LiveOutputUpdate,
};

use super::producer;

#[test]
fn snapshot_then_batch_preserves_text_tool_text_order() {
    let store = LiveOutputStore::default();
    store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 1,
            items: vec![LiveOutputItem::AssistantText {
                item_id: "text_1".into(),
                text: "hello".into(),
            }],
        })
        .unwrap();

    let outcome = store
        .publish_batch(LiveOutputBatch {
            producer: producer(),
            first_sequence: 2,
            updates: vec![
                LiveOutputUpdate::ToolCall {
                    item_id: "tool_1".into(),
                    call_id: "call_1".into(),
                    tool_name: "read".into(),
                    arguments: serde_json::json!({"path": "README.md"}),
                    managed_tool_use: None,
                },
                LiveOutputUpdate::AssistantTextDelta {
                    item_id: "text_2".into(),
                    delta: "world".into(),
                },
            ],
        })
        .unwrap();

    assert_eq!(
        outcome,
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence: 3,
            duplicate: false,
        }
    );
    assert_eq!(
        store.snapshot("sess_1", "turn_1").unwrap().items,
        vec![
            LiveOutputItem::AssistantText {
                item_id: "text_1".into(),
                text: "hello".into(),
            },
            LiveOutputItem::ToolCall {
                item_id: "tool_1".into(),
                call_id: "call_1".into(),
                tool_name: "read".into(),
                arguments: serde_json::json!({"path": "README.md"}),
                managed_tool_use: None,
            },
            LiveOutputItem::AssistantText {
                item_id: "text_2".into(),
                text: "world".into(),
            },
        ]
    );
}

#[test]
fn duplicate_batch_is_idempotent_and_gap_requests_snapshot() {
    let store = LiveOutputStore::default();
    store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 1,
            items: vec![LiveOutputItem::AssistantText {
                item_id: "text_1".into(),
                text: "a".into(),
            }],
        })
        .unwrap();
    let batch = LiveOutputBatch {
        producer: producer(),
        first_sequence: 2,
        updates: vec![LiveOutputUpdate::AssistantTextDelta {
            item_id: "text_1".into(),
            delta: "b".into(),
        }],
    };
    store.publish_batch(batch.clone()).unwrap();
    assert_eq!(
        store.publish_batch(batch).unwrap(),
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence: 2,
            duplicate: true,
        }
    );
    assert_eq!(
        store
            .publish_batch(LiveOutputBatch {
                producer: producer(),
                first_sequence: 2,
                updates: vec![LiveOutputUpdate::AssistantTextDelta {
                    item_id: "text_1".into(),
                    delta: "conflict".into(),
                }],
            })
            .unwrap(),
        LiveOutputPublishOutcome::SnapshotRequired {
            accepted_sequence: 2,
        }
    );
    assert_eq!(
        store
            .publish_batch(LiveOutputBatch {
                producer: producer(),
                first_sequence: 4,
                updates: vec![LiveOutputUpdate::AssistantTextDelta {
                    item_id: "text_1".into(),
                    delta: "gap".into(),
                }],
            })
            .unwrap(),
        LiveOutputPublishOutcome::SnapshotRequired {
            accepted_sequence: 2,
        }
    );
    assert_eq!(
        store.snapshot("sess_1", "turn_1").unwrap().items,
        vec![LiveOutputItem::AssistantText {
            item_id: "text_1".into(),
            text: "ab".into(),
        }]
    );
}

#[test]
fn another_stream_cannot_replace_an_active_turn_stream() {
    let store = LiveOutputStore::default();
    store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 1,
            items: Vec::new(),
        })
        .unwrap();
    let mut replacement_producer = producer();
    replacement_producer.identity.stream_id = "stream_2".into();

    let error = store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: replacement_producer,
            sequence: 1,
            items: Vec::new(),
        })
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("already has an active live output stream")
    );
    assert_eq!(
        store
            .snapshot("sess_1", "turn_1")
            .unwrap()
            .identity
            .stream_id,
        "stream_1"
    );
}
