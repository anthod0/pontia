use super::*;
use pontia_bot::helpers::HelperError;

fn compact(caller: &str, target: &str) -> HelperRequest {
    HelperRequest {
        session_id: caller.into(),
        command: HelperCommand::Compact {
            session_id: target.into(),
        },
    }
}

#[tokio::test]
async fn compaction_requires_the_current_bot_and_its_own_helper() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (_, owner) = bot(&conversation).await;
    let (_, other) = bot(&conversation).await;
    let target = helper(&helpers, &owner).await;
    finish(&control, &target, EventType::TurnCompleted).await;

    for (caller, target) in [
        (other.as_str(), target.as_str()),
        (target.as_str(), target.as_str()),
        (owner.as_str(), owner.as_str()),
        (owner.as_str(), "missing"),
    ] {
        assert!(matches!(
            helpers.execute(compact(caller, target)).await,
            Err(HelperError::Control(Error::Conflict {
                code: "bot_session_required",
                ..
            }))
        ));
    }
}

#[tokio::test]
async fn compaction_preserves_busy_exited_and_unsupported_errors() {
    let (_root, _pool, _client, control, conversation) = fixture().await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (_, owner) = bot(&conversation).await;
    let target = helper(&helpers, &owner).await;
    let snapshot = control
        .queries()
        .session_snapshot(&target)
        .await
        .unwrap()
        .unwrap();
    let turn = control
        .queries()
        .list_turns(&target)
        .await
        .unwrap()
        .pop()
        .unwrap();
    report(
        &control.event_ingest_service(),
        &target,
        &snapshot.runtimes[0].runtime_id,
        &turn.turn_id,
        EventType::TurnStarted,
    )
    .await;
    assert!(matches!(
        helpers.execute(compact(&owner, &target)).await,
        Err(HelperError::Control(Error::StateConflict(_)))
    ));
    finish(&control, &target, EventType::TurnCompleted).await;
    assert!(matches!(
        helpers.execute(compact(&owner, &target)).await,
        Err(HelperError::Control(Error::CapabilityUnavailable(_)))
    ));
    control
        .event_ingest_service()
        .report_client_fact(
            &target,
            Some(&snapshot.runtimes[0].runtime_id),
            EventType::SessionExited,
            json!({}),
        )
        .await
        .unwrap();
    assert!(matches!(
        helpers.execute(compact(&owner, &target)).await,
        Err(HelperError::Control(Error::StateConflict(_)))
    ));
}
