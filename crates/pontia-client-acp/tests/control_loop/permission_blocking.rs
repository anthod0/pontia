use super::*;
use pontia_application::interactions::InteractionState;

impl Harness {
    pub(super) async fn wait_session_state(&self, session: &str, expected: &str) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if self
                    .app
                    .queries()
                    .get_session(session)
                    .await
                    .unwrap()
                    .unwrap()
                    .state
                    == expected
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("Session did not become {expected}"));
    }

    async fn wait_permission_facts(
        &self,
        session: &str,
        count: usize,
    ) -> Vec<(String, String, String)> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let facts = self.permission_facts(session).await;
                if facts.len() == count {
                    return facts;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("permission facts did not persist")
    }

    async fn permission_facts(&self, session: &str) -> Vec<(String, String, String)> {
        sqlx::query_as("SELECT event_type, json_extract(payload,'$.interaction_id'), turn_id FROM events WHERE session_id=? AND event_type IN ('interaction.blocked','interaction.unblocked') ORDER BY rowid")
            .bind(session).fetch_all(&self.app.db()).await.unwrap()
    }
}

#[tokio::test]
async fn fact_ingestion_retries_without_delaying_or_replaying_permission_delivery() {
    for failing_kind in ["interaction.blocked", "interaction.unblocked"] {
        let h = Harness::new(&[("ACP_PERMISSION", "1"), ("ACP_REPLY_BACKPRESSURE", "1")]).await;
        let session = h.create("first").await.unwrap();
        let turn = h
            .app
            .turn_commands()
            .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        if failing_kind == "interaction.blocked" {
            sqlx::query("CREATE TRIGGER fail_permission_fact BEFORE INSERT ON events WHEN NEW.event_type='interaction.blocked' BEGIN SELECT RAISE(ABORT, 'temporary event outage'); END").execute(&h.app.db()).await.unwrap();
        }
        h.release("update-1");
        let record = h.wait_interactions(&session, 1).await.remove(0);
        if failing_kind == "interaction.unblocked" {
            h.wait_permission_facts(&session, 1).await;
            sqlx::query("CREATE TRIGGER fail_permission_fact BEFORE INSERT ON events WHEN NEW.event_type='interaction.unblocked' BEGIN SELECT RAISE(ABORT, 'temporary event outage'); END").execute(&h.app.db()).await.unwrap();
        }
        h.release("backpressure-1");
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            h.app.interactions().respond(
                &h.app,
                &session,
                &record.interaction_id,
                &"x".repeat(262144),
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.state, InteractionState::Resolved);
        assert_eq!(h.events(&session, failing_kind).await, 0);
        sqlx::query("DROP TRIGGER fail_permission_fact")
            .execute(&h.app.db())
            .await
            .unwrap();
        let facts = h.wait_permission_facts(&session, 2).await;
        assert_eq!(
            facts,
            [
                (
                    "interaction.blocked".into(),
                    record.interaction_id.clone(),
                    turn.turn_id.clone()
                ),
                (
                    "interaction.unblocked".into(),
                    record.interaction_id.clone(),
                    turn.turn_id.clone()
                )
            ]
        );
        h.wait_session_state(&session, "busy").await;
        h.app
            .interactions()
            .respond(
                &h.app,
                &session,
                &record.interaction_id,
                &"x".repeat(262144),
            )
            .await
            .unwrap();
        h.release("release-1");
        h.wait_turn(&session, &turn.turn_id, "completed").await;
        assert_eq!(
            h.wire()
                .iter()
                .filter(|frame| frame["result"]["outcome"]["outcome"] == "selected")
                .count(),
            1
        );
        h.stop().await;
    }
}

#[tokio::test]
async fn selected_response_failure_never_reports_unblocked_or_replays_response() {
    let h = Harness::new(&[
        ("ACP_PERMISSION", "1"),
        ("ACP_REPLY_BACKPRESSURE", "1"),
        ("ACP_BREAK_REPLY", "1"),
    ])
    .await;
    let session = h.create("first").await.unwrap();
    h.app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    let record = h.wait_interactions(&session, 1).await.remove(0);
    let responding = tokio::spawn({
        let app = h.app.clone();
        let session = session.clone();
        let id = record.interaction_id.clone();
        async move {
            app.interactions()
                .respond(&app, &session, &id, &"x".repeat(262144))
                .await
        }
    });
    h.wait_interaction_state(
        &session,
        &record.interaction_id,
        InteractionState::Resolving,
    )
    .await;
    h.release("backpressure-1");
    assert!(
        tokio::time::timeout(Duration::from_secs(5), responding)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    h.wait_runtime_exited(&session).await;
    h.wait_interaction_state(&session, &record.interaction_id, InteractionState::Expired)
        .await;
    assert_eq!(h.events(&session, "interaction.unblocked").await, 0);
    assert_eq!(h.events(&session, "interaction.blocked").await, 1);
    assert!(!h.wire().iter().any(|frame| frame.get("result").is_some()));
    h.stop().await;
}

#[tokio::test]
async fn prompt_terminal_before_selection_flush_is_not_overwritten_by_late_unblock() {
    terminal_before_flush(false).await;
}

#[tokio::test]
async fn cancelling_the_response_caller_does_not_lose_the_flush_association() {
    terminal_before_flush(true).await;
}

async fn terminal_before_flush(cancel_caller: bool) {
    let h = Harness::new(&[
        ("ACP_PERMISSION", "1"),
        ("ACP_REPLY_BACKPRESSURE", "1"),
        ("ACP_RESULT_BEFORE_DRAIN", "1"),
    ])
    .await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    let record = h.wait_interactions(&session, 1).await.remove(0);
    let responding = tokio::spawn({
        let app = h.app.clone();
        let session = session.clone();
        let id = record.interaction_id.clone();
        async move {
            app.interactions()
                .respond(&app, &session, &id, &"x".repeat(262144))
                .await
        }
    });
    h.wait_interaction_state(
        &session,
        &record.interaction_id,
        InteractionState::Resolving,
    )
    .await;
    h.release("backpressure-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.wait_session_state(&session, "idle").await;
    assert_eq!(h.events(&session, "interaction.unblocked").await, 0);
    if cancel_caller {
        responding.abort();
        assert!(responding.await.unwrap_err().is_cancelled());
        h.release("drain-result-1");
    } else {
        h.release("drain-result-1");
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), responding)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .state,
            InteractionState::Resolved
        );
    }
    h.wait_permission_facts(&session, 2).await;
    assert_eq!(h.events(&session, "interaction.unblocked").await, 1);
    h.wait_session_state(&session, "idle").await;
    h.stop().await;
}

#[tokio::test]
async fn each_same_tool_permission_unblocks_on_selection_without_waiting_for_prompt_result() {
    let h = Harness::new(&[("ACP_PERMISSION", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    let records = h.wait_interactions(&session, 3).await;
    h.wait_turn(&session, &turn.turn_id, "running").await;
    h.wait_session_state(&session, "waiting_for_interaction")
        .await;
    let blocked = h.wait_permission_facts(&session, 3).await;
    assert_eq!(blocked.len(), 3);
    for record in &records {
        assert!(blocked.contains(&(
            "interaction.blocked".into(),
            record.interaction_id.clone(),
            turn.turn_id.clone()
        )));
    }
    for (index, record) in records.iter().enumerate() {
        let option = if index == 1 {
            "reject-once"
        } else {
            "once:原"
        };
        for _ in 0..2 {
            h.app
                .interactions()
                .respond(&h.app, &session, &record.interaction_id, option)
                .await
                .unwrap();
        }
        h.wait_session_state(
            &session,
            if index == 2 {
                "busy"
            } else {
                "waiting_for_interaction"
            },
        )
        .await;
        let facts = h.wait_permission_facts(&session, 4 + index).await;
        assert_eq!(facts.len(), 4 + index);
        assert_eq!(
            facts.last().unwrap(),
            &(
                "interaction.unblocked".into(),
                record.interaction_id.clone(),
                turn.turn_id.clone()
            )
        );
        assert_eq!(h.events(&session, "turn.completed").await, 0);
    }
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.wait_session_state(&session, "idle").await;
    h.wait_requests("session/prompt", 1).await;
    // A repeated public response never replays the native RPC response.
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if h.wire()
                .iter()
                .filter(|frame| frame["result"]["outcome"]["outcome"] == "selected")
                .count()
                == 3
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    h.stop().await;
}

#[tokio::test]
async fn native_cancel_does_not_prove_unblocked_and_prompt_terminal_clears_remaining_waits() {
    let h = Harness::new(&[("ACP_PERMISSION", "1"), ("ACP_NATIVE_CANCEL", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    let records = h.wait_interactions(&session, 3).await;
    h.wait_session_state(&session, "waiting_for_interaction")
        .await;
    h.release("native-cancel-1");
    let cancelled = records
        .iter()
        .find(|record| {
            record
                .description
                .as_ref()
                .unwrap()
                .ends_with("request:unanswered-1")
        })
        .unwrap();
    h.wait_interaction_state(
        &session,
        &cancelled.interaction_id,
        InteractionState::Expired,
    )
    .await;
    for record in records
        .iter()
        .filter(|record| record.interaction_id != cancelled.interaction_id)
    {
        h.app
            .interactions()
            .respond(&h.app, &session, &record.interaction_id, "reject-always")
            .await
            .unwrap();
    }
    h.wait_session_state(&session, "waiting_for_interaction")
        .await;
    h.wait_permission_facts(&session, 5).await;
    assert_eq!(h.events(&session, "interaction.unblocked").await, 2);
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "completed").await;
    h.wait_session_state(&session, "idle").await;
    assert_eq!(h.events(&session, "interaction.unblocked").await, 2);
    h.stop().await;
}

#[tokio::test]
async fn cancelled_permission_responses_do_not_restore_busy_before_prompt_stops() {
    let h = Harness::new(&[("ACP_PERMISSION", "1")]).await;
    let session = h.create("first").await.unwrap();
    let turn = h
        .app
        .turn_commands()
        .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
        .await
        .unwrap()
        .unwrap();
    h.release("update-1");
    h.wait_interactions(&session, 3).await;
    h.wait_session_state(&session, "waiting_for_interaction")
        .await;
    h.app
        .turn_commands()
        .interrupt_turn(&session, &turn.turn_id)
        .await
        .unwrap();
    h.wait_session_state(&session, "waiting_for_interaction")
        .await;
    assert_eq!(h.events(&session, "interaction.unblocked").await, 0);
    assert_eq!(h.events(&session, "turn.interrupted").await, 0);
    h.release("release-1");
    h.wait_turn(&session, &turn.turn_id, "interrupted").await;
    h.wait_session_state(&session, "interrupted").await;
    h.stop().await;
}

#[tokio::test]
async fn disconnect_or_close_never_synthesizes_successful_selection() {
    for close in [false, true] {
        let h = Harness::new(&[("ACP_PERMISSION", "1"), ("ACP_DROP_PROMPT", "1")]).await;
        let session = h.create("first").await.unwrap();
        h.app
            .turn_commands()
            .create_and_dispatch_turn(&session, "end_turn".into(), json!({}))
            .await
            .unwrap()
            .unwrap();
        h.release("update-1");
        let records = h.wait_interactions(&session, 3).await;
        h.wait_session_state(&session, "waiting_for_interaction")
            .await;
        if close {
            h.app
                .session_commands()
                .terminate_session(&session)
                .await
                .unwrap();
        } else {
            h.release("release-1");
        }
        h.wait_runtime_exited(&session).await;
        for record in records {
            h.wait_interaction_state(&session, &record.interaction_id, InteractionState::Expired)
                .await;
        }
        h.wait_session_state(
            &session,
            if close {
                "exited"
            } else {
                "waiting_for_interaction"
            },
        )
        .await;
        assert_eq!(h.events(&session, "interaction.unblocked").await, 0);
        h.stop().await;
    }
}
