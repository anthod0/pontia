use std::{collections::VecDeque, time::Instant};

use crate::live_output::{
    LiveOutputClose, LiveOutputItem, LiveOutputPublishOutcome, LiveOutputSnapshotReplacement,
    LiveOutputStore,
};

use super::super::{ACTIVE_STREAM_TTL, StreamState, stream_key};
use super::{identity, producer};

#[test]
fn close_is_idempotent_and_hides_the_snapshot() {
    let store = LiveOutputStore::default();
    store
        .replace_snapshot(LiveOutputSnapshotReplacement {
            producer: producer(),
            sequence: 1,
            items: vec![LiveOutputItem::AssistantText {
                item_id: "text_1".into(),
                text: "done".into(),
            }],
        })
        .unwrap();
    let close = LiveOutputClose {
        producer: producer(),
        sequence: 2,
    };

    assert_eq!(
        store.close(close.clone()).unwrap(),
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence: 2,
            duplicate: false,
        }
    );
    assert!(store.snapshot("sess_1", "turn_1").is_none());
    assert_eq!(
        store.close(close).unwrap(),
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence: 2,
            duplicate: true,
        }
    );
}

#[test]
fn expired_streams_are_removed_lazily() {
    let store = LiveOutputStore::default();
    store.lock().streams.insert(
        stream_key(&identity()),
        StreamState {
            sequence: 1,
            items: Vec::new(),
            accepted_updates: VecDeque::new(),
            closed: false,
            updated_at: Instant::now() - ACTIVE_STREAM_TTL,
        },
    );

    assert!(store.snapshot("sess_1", "turn_1").is_none());
    assert!(store.lock().streams.is_empty());
}
