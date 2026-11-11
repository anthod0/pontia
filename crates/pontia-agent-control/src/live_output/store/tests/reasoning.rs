use super::producer;
use crate::live_output::*;
use serde_json::{Value, json};

const BUDGET: usize = 1024 * 1024;

fn store(items: Value) -> LiveOutputStore {
    let store = LiveOutputStore::default();
    store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 1,
            items: serde_json::from_value(items).unwrap(),
        })
        .unwrap();
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

fn current(store: &LiveOutputStore) -> Vec<LiveOutputItem> {
    store.snapshot("sess_1", "turn_1").unwrap().items
}

fn reasoning(item_id: &str, delta: &str) -> LiveOutputUpdate {
    LiveOutputUpdate::ReasoningDelta {
        item_id: item_id.into(),
        delta: delta.into(),
        truncated: false,
    }
}

#[test]
fn reasoning_deltas_accumulate_per_item_in_order_with_tools_and_text() {
    let store = store(json!([]));
    store
        .publish_batch(batch(
            2,
            json!([
                {"type":"reasoning_delta","item_id":"r1","delta":"Inspect"},
                {"type":"tool_call","item_id":"t1","call_id":"c1","tool_name":"bash","arguments":{}},
                {"type":"reasoning_delta","item_id":"r1","delta":"ing files"},
                {"type":"reasoning_delta","item_id":"r2","delta":"Next","truncated":true},
                {"type":"assistant_text_delta","item_id":"a1","delta":"done"},
                {"type":"reasoning_delta","item_id":"r2","delta":" step"}
            ]),
        ))
        .unwrap();
    let value = items(&store);
    assert_eq!(
        value[0],
        json!({"kind":"reasoning","item_id":"r1","text":"Inspecting files","truncated":false})
    );
    assert_eq!(value[1]["call_id"], "c1");
    assert_eq!(
        value[2],
        json!({"kind":"reasoning","item_id":"r2","text":"Next step","truncated":true})
    );
    assert_eq!(value[3]["text"], "done");
}

#[test]
fn reasoning_delta_on_an_item_of_another_kind_is_rejected_without_changes() {
    let store = store(json!([
        {"kind":"assistant_text","item_id":"a1","text":"hello"},
        {"kind":"reasoning","item_id":"r1","text":"think"}
    ]));
    let before = items(&store);
    for updates in [
        json!([{"type":"reasoning_delta","item_id":"a1","delta":"x"}]),
        json!([{"type":"assistant_text_delta","item_id":"r1","delta":"x"}]),
        json!([{"type":"reasoning_delta","item_id":"r1","delta":""}]),
    ] {
        assert!(store.publish_batch(batch(2, updates)).is_err());
    }
    assert_eq!(items(&store), before);
    assert_eq!(store.snapshot("sess_1", "turn_1").unwrap().sequence, 1);
}

#[test]
fn reasoning_budget_is_independent_of_assistant_text_and_enforced_by_the_store() {
    let full_text = "t".repeat(BUDGET);
    let full_reasoning = "r".repeat(BUDGET);
    let store = store(json!([
        {"kind":"assistant_text","item_id":"a1","text":full_text},
        {"kind":"reasoning","item_id":"r1","text":full_reasoning}
    ]));
    assert!(
        store
            .publish_batch(LiveOutputBatch {
                producer: producer(),
                first_sequence: 2,
                updates: vec![reasoning("r2", "x")],
            })
            .is_err()
    );
    assert!(
        store
            .replace_snapshot(LiveOutputSnapshotReplacement {
                producer: producer(),
                sequence: 3,
                items: serde_json::from_value(json!([
                    {"kind":"reasoning","item_id":"r1","text":"r".repeat(BUDGET / 2 + 1)},
                    {"kind":"reasoning","item_id":"r2","text":"r".repeat(BUDGET / 2)}
                ]))
                .unwrap(),
            })
            .is_err()
    );
    assert_eq!(store.snapshot("sess_1", "turn_1").unwrap().sequence, 1);
}

#[test]
fn producer_truncates_reasoning_at_the_budget_and_drops_later_reasoning_only() {
    let store = store(json!([{"kind":"reasoning","item_id":"r1","text":"r".repeat(BUDGET - 4)}]));
    // A multibyte character must not be split at the budget boundary.
    let LiveReasoningBound::Update(bounded) =
        reasoning("r1", "ab界cd").bound_reasoning(&current(&store))
    else {
        panic!("reasoning within the budget must be published");
    };
    assert_eq!(
        bounded,
        LiveOutputUpdate::ReasoningDelta {
            item_id: "r1".into(),
            delta: "ab".into(),
            truncated: true,
        }
    );
    store
        .publish_batch(LiveOutputBatch {
            producer: producer(),
            first_sequence: 2,
            updates: vec![bounded],
        })
        .unwrap();
    for later in [reasoning("r1", "x"), reasoning("r2", "x")] {
        assert_eq!(
            later.bound_reasoning(&current(&store)),
            LiveReasoningBound::Drop
        );
    }
    let text = LiveOutputUpdate::AssistantTextDelta {
        item_id: "a1".into(),
        delta: "still visible".into(),
    };
    assert_eq!(
        text.clone().bound_reasoning(&current(&store)),
        LiveReasoningBound::Update(text.clone())
    );
    store
        .publish_batch(LiveOutputBatch {
            producer: producer(),
            first_sequence: 3,
            updates: vec![text],
        })
        .unwrap();
    let value = items(&store);
    assert_eq!(value[0]["truncated"], true);
    assert_eq!(value[0]["text"].as_str().unwrap().len(), BUDGET - 2);
    assert_eq!(value[1]["text"], "still visible");
}

#[test]
fn exhausted_reasoning_budget_marks_existing_reasoning_truncated_through_a_snapshot() {
    let store = store(json!([
        {"kind":"reasoning","item_id":"r1","text":"r".repeat(BUDGET / 2)},
        {"kind":"tool_call","item_id":"t1","call_id":"c1","tool_name":"bash","arguments":{}},
        {"kind":"reasoning","item_id":"r2","text":"r".repeat(BUDGET / 2)}
    ]));
    let LiveReasoningBound::Snapshot(replacement) =
        reasoning("r3", "more").bound_reasoning(&current(&store))
    else {
        panic!("an exhausted budget must be reported through a snapshot");
    };
    store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 2,
            items: replacement,
        })
        .unwrap();
    let value = items(&store);
    assert_eq!(value.as_array().unwrap().len(), 3);
    assert_eq!(value[0]["truncated"], false);
    assert_eq!(value[2]["truncated"], true);
    assert_eq!(
        reasoning("r3", "more").bound_reasoning(&current(&store)),
        LiveReasoningBound::Drop
    );
}
