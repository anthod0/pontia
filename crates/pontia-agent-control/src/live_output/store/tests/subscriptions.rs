use std::time::Duration;

use tokio::sync::broadcast;

use crate::live_output::{
    LiveOutputBatch, LiveOutputItem, LiveOutputPublishOutcome, LiveOutputSnapshotReplacement,
    LiveOutputStore, LiveOutputStreamEvent, LiveOutputUpdate,
};

use super::super::SUBSCRIBER_CAPACITY;
use super::producer;

#[tokio::test]
async fn subscription_starts_with_an_atomic_snapshot_then_receives_updates() {
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

    let mut subscription = store.subscribe_session("sess_1");
    assert_eq!(subscription.initial_snapshot.as_ref().unwrap().sequence, 1);
    store
        .publish_batch(LiveOutputBatch {
            producer: producer(),
            first_sequence: 2,
            updates: vec![LiveOutputUpdate::AssistantTextDelta {
                item_id: "text_1".into(),
                delta: "b".into(),
            }],
        })
        .unwrap();

    assert!(matches!(
        subscription.recv().await.unwrap(),
        LiveOutputStreamEvent::Updates {
            first_sequence: 2,
            ..
        }
    ));
}

#[tokio::test]
async fn retries_do_not_publish_and_session_discard_notifies_subscribers() {
    let store = LiveOutputStore::default();
    let replacement = LiveOutputSnapshotReplacement {
        producer: producer(),
        sequence: 1,
        items: Vec::new(),
    };
    store.replace_snapshot(replacement.clone()).unwrap();
    let mut subscription = store.subscribe_session("sess_1");
    assert!(subscription.initial_snapshot.take().is_some());

    assert_eq!(
        store.replace_snapshot(replacement).unwrap(),
        LiveOutputPublishOutcome::Accepted {
            accepted_sequence: 1,
            duplicate: true,
        }
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(10), subscription.recv())
            .await
            .is_err()
    );

    store.discard_session("sess_1");
    assert!(matches!(
        subscription.recv().await.unwrap(),
        LiveOutputStreamEvent::Closed { sequence: 1, .. }
    ));
}

#[tokio::test]
async fn lagged_subscriber_can_recover_from_the_current_snapshot() {
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
    let mut subscription = store.subscribe_session("sess_1");

    for sequence in 2..=(SUBSCRIBER_CAPACITY as u64 + 2) {
        store
            .publish_batch(LiveOutputBatch {
                producer: producer(),
                first_sequence: sequence,
                updates: vec![LiveOutputUpdate::AssistantTextDelta {
                    item_id: "text_1".into(),
                    delta: "x".into(),
                }],
            })
            .unwrap();
    }

    assert!(matches!(
        subscription.recv().await,
        Err(broadcast::error::RecvError::Lagged(_))
    ));
    assert_eq!(
        store
            .subscribe_session("sess_1")
            .initial_snapshot
            .unwrap()
            .sequence,
        SUBSCRIBER_CAPACITY as u64 + 2
    );
}
