use super::producer;
use crate::live_output::*;
use serde_json::{Value, json};

fn store() -> LiveOutputStore {
    let store = LiveOutputStore::default();
    store.replace_snapshot(LiveOutputSnapshotReplacement { producer: producer(), sequence: 1, items: serde_json::from_value(json!([
        {"kind":"tool_call","item_id":"one","call_id":"one","tool_name":"bash","arguments":{"command":"first"}},
        {"kind":"tool_call","item_id":"two","call_id":"two","tool_name":"bash","arguments":{"command":"second"}}
    ])).unwrap() }).unwrap();
    store
}
fn batch(sequence: u64, updates: Value) -> LiveOutputBatch {
    LiveOutputBatch {
        producer: producer(),
        first_sequence: sequence,
        updates: serde_json::from_value(updates).unwrap(),
    }
}
fn items(store: &LiveOutputStore) -> Value {
    serde_json::to_value(store.snapshot("sess_1", "turn_1").unwrap().items).unwrap()
}

#[test]
fn interleaved_outputs_results_and_status_restore_without_duplicates() {
    let store = store();
    let updates = batch(
        2,
        json!([
            {"type":"tool_status_updated","call_id":"one","status":{"phase":"running"}},
            {"type":"tool_output_delta","call_id":"one","delta":"a"},
            {"type":"tool_output_updated","call_id":"two","text":"x"},
            {"type":"tool_output_delta","call_id":"one","delta":"b"},
            {"type":"tool_output_updated","call_id":"two","text":"xy","truncated":true},
            {"type":"tool_result","call_id":"one","result":{"content":"ab"}},
            {"type":"tool_status_updated","call_id":"one","status":{"phase":"ended","outcome":"succeeded"}},
            {"type":"tool_result","call_id":"two","result":{"error":{"message":"failed"}}},
            {"type":"tool_status_updated","call_id":"two","status":{"phase":"ended","outcome":"failed"}}
        ]),
    );
    store.publish_batch(updates.clone()).unwrap();
    assert!(matches!(
        store.publish_batch(updates).unwrap(),
        LiveOutputPublishOutcome::Accepted {
            duplicate: true,
            ..
        }
    ));
    let snapshot = store.subscribe_session("sess_1").initial_snapshot.unwrap();
    assert_eq!(snapshot.items.len(), 2);
    let value = serde_json::to_value(snapshot.items).unwrap();
    assert_eq!(value[0]["output"], json!({"text":"ab","truncated":false}));
    assert_eq!(value[0]["result"]["content"], "ab");
    assert_eq!(
        value[0]["status"],
        json!({"phase":"ended","outcome":"succeeded"})
    );
    assert_eq!(value[1]["output"], json!({"text":"xy","truncated":true}));
    assert_eq!(value[1]["result"]["error"]["message"], "failed");
    assert_eq!(value[1]["status"]["outcome"], "failed");
}

#[test]
fn late_starts_and_snapshot_discovery_do_not_erase_known_outcomes() {
    let store = store();
    store.publish_batch(batch(2, json!([
        {"type":"tool_status_updated","call_id":"one","status":{"phase":"ended"}},
        {"type":"tool_status_updated","call_id":"one","status":{"phase":"ended","outcome":"rejected"}},
        {"type":"tool_status_updated","call_id":"one","status":{"phase":"running"}},
        {"type":"tool_result","call_id":"one","result":{"content":null}}
    ]))).unwrap();
    assert_eq!(items(&store)[0]["status"]["outcome"], "rejected");
    let mut replacement = store.snapshot("sess_1", "turn_1").unwrap().items;
    if let LiveOutputItem::ToolCall { status, result, .. } = &mut replacement[0] {
        *status = None;
        *result = None;
    }
    store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 6,
            items: replacement,
        })
        .unwrap();
    assert_eq!(items(&store)[0]["status"]["outcome"], "rejected");
    assert_eq!(
        items(&store)[0]["result"],
        json!({"content":null,"truncated":false})
    );
}

#[test]
fn replacement_output_replay_cache_is_byte_bounded() {
    let store = store();
    let first = batch(
        2,
        json!([{ "type":"tool_output_updated","call_id":"one","text":"a".repeat(900*1024) }]),
    );
    store.publish_batch(first.clone()).unwrap();
    let latest = batch(
        3,
        json!([{ "type":"tool_output_updated","call_id":"one","text":"b".repeat(900*1024) }]),
    );
    store.publish_batch(latest.clone()).unwrap();
    assert!(matches!(
        store.publish_batch(first).unwrap(),
        LiveOutputPublishOutcome::SnapshotRequired {
            accepted_sequence: 3
        }
    ));
    assert!(matches!(
        store.publish_batch(latest).unwrap(),
        LiveOutputPublishOutcome::Accepted {
            duplicate: true,
            ..
        }
    ));
    assert_eq!(items(&store)[0]["output"]["text"], "b".repeat(900 * 1024));
}

#[test]
fn snapshot_omission_does_not_erase_observed_output() {
    let store = store();
    let calls = store.snapshot("sess_1", "turn_1").unwrap().items;
    store
        .publish_batch(batch(
            2,
            json!([{ "type":"tool_output_delta","call_id":"one","delta":"observed" }]),
        ))
        .unwrap();
    store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 3,
            items: calls,
        })
        .unwrap();
    assert_eq!(items(&store)[0]["output"]["text"], "observed");
}

#[test]
fn invalid_execution_updates_reject_the_whole_batch() {
    for invalid in [
        json!({"type":"tool_output_delta","call_id":"missing","delta":"lost"}),
        json!({"type":"tool_status_updated","call_id":"one","status":{"phase":"running","outcome":"succeeded"}}),
        json!({"type":"tool_result","call_id":"one","result":{}}),
        json!({"type":"tool_output_updated","call_id":"one","text":"x".repeat(1024*1024)}),
    ] {
        let store = store();
        let before = items(&store);
        assert!(store.publish_batch(batch(2, json!([
            {"type":"tool_output_delta","call_id":"two","delta":"must not leak"}, invalid
        ]))).is_err());
        assert_eq!(items(&store), before);
        assert_eq!(store.snapshot("sess_1", "turn_1").unwrap().sequence, 1);
    }
}

#[test]
fn producer_bounds_utf8_content_and_can_still_publish_end_state() {
    let store = store();
    let update = LiveOutputUpdate::ToolOutputDelta {
        call_id: "one".into(),
        delta: "界".repeat(1024 * 1024),
        truncated: false,
    }
    .bound_tool_content(&store.snapshot("sess_1", "turn_1").unwrap().items);
    store
        .publish_batch(LiveOutputBatch {
            producer: producer(),
            first_sequence: 2,
            updates: vec![update],
        })
        .unwrap();
    let result = LiveOutputUpdate::ToolResult {
        call_id: "one".into(),
        result: LiveToolResult {
            content: Some(json!({"large":"界".repeat(1024*1024)})),
            error: None,
            truncated: false,
        },
    }
    .bound_tool_content(&store.snapshot("sess_1", "turn_1").unwrap().items);
    store
        .publish_batch(LiveOutputBatch {
            producer: producer(),
            first_sequence: 3,
            updates: vec![result],
        })
        .unwrap();
    store.publish_batch(batch(4, json!([{ "type":"tool_status_updated","call_id":"one","status":{"phase":"ended","outcome":"interrupted"}}]))).unwrap();
    let value = items(&store);
    assert_eq!(value[0]["output"]["truncated"], true);
    assert_eq!(value[0]["result"]["truncated"], true);
    assert_eq!(value[0]["status"]["outcome"], "interrupted");
}
