use super::*;
use pontia_agent_control::workspaces::{RegisterWorkspaceRequest, WorkspaceService};
use pontia_bot::{
    messages::TodoAction,
    workspaces::{WorkspaceCommand, WorkspaceCommands, WorkspaceRequest},
};
use std::time::Duration;

fn workspaces(root: &Path, control: &AgentControl) -> WorkspaceService {
    WorkspaceService::new(
        control.db(),
        serde_json::from_value(json!({"roots":[{
            "root_id":"root", "label":"Test", "path":root
        }]}))
        .unwrap(),
        control.control_changes(),
    )
}

fn register(path: &str) -> RegisterWorkspaceRequest {
    RegisterWorkspaceRequest {
        root_id: "root".into(),
        path: path.into(),
        name: None,
        description: Some("project".into()),
    }
}

async fn todo(service: &ConversationService, session: &str, text: &str) -> String {
    service
        .send_to_user(ReplyRequest {
            session_id: session.into(),
            input: Reply {
                message: text.into(),
                todo: true,
                related_to: vec![],
                resolve_todos: vec![],
            },
        })
        .await
        .unwrap()
        .message
        .id
}

async fn wait_for_inputs(client: &Client, count: usize) {
    let timeout = std::time::Instant::now() + Duration::from_secs(10);
    while client.inputs.lock().unwrap().len() < count {
        assert!(
            std::time::Instant::now() < timeout,
            "Timed out waiting for {count} inputs"
        );
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn fixed_windows_merge_by_last_occurrence_and_send_only_to_live_recipients() {
    let (root, pool, client, control, service) = fixture().await;
    let ws = workspaces(root.path(), &control);
    ws.register(register(""), None).await.unwrap();
    let (id, active) = bot(&service).await;
    let (_, idle) = bot(&service).await;
    finish(&control, &idle, EventType::TurnCompleted).await;
    let (_, exited) = bot(&service).await;
    let never = service
        .records()
        .create(bot_configuration("Never started", MODEL))
        .await
        .unwrap();
    let task = todo(&service, &active, &format!("One\n{}", "字".repeat(100))).await;
    let runtime = control
        .queries()
        .session_snapshot(&active)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    let turns = control.queries().list_turns(&active).await.unwrap();
    report(
        &control.event_ingest_service(),
        &active,
        &runtime,
        &turns[0].turn_id,
        EventType::TurnStarted,
    )
    .await;

    let (stop, receiver) = tokio::sync::watch::channel(false);
    let notices = tokio::spawn(service.system_notices().run(receiver));
    let manual_clock = tokio::spawn(async {
        loop {
            tokio::task::yield_now().await;
        }
    });
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(4)).await;
    let before = client.inputs.lock().unwrap().len();
    assert_eq!(before, 3);
    service
        .change_todo(&id, &task, TodoAction::Resolve)
        .await
        .unwrap();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(1)).await;
    std::fs::create_dir(root.path().join("new")).unwrap();
    let new = ws.register(register("new"), None).await.unwrap();
    ws.register(register("new"), None).await.unwrap();
    control
        .agent_profiles()
        .create(profile("reviewer", MODEL, None))
        .await
        .unwrap();
    service
        .change_todo(&id, &task, TodoAction::Reopen)
        .await
        .unwrap();
    control
        .event_ingest_service()
        .report_client_fact(
            &exited,
            Some(
                &control
                    .queries()
                    .session_snapshot(&exited)
                    .await
                    .unwrap()
                    .unwrap()
                    .runtimes[0]
                    .runtime_id,
            ),
            EventType::SessionExited,
            json!({"reason":"test exit"}),
        )
        .await
        .unwrap();
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_millis(1999)).await;
    assert_eq!(client.inputs.lock().unwrap().len(), before);
    tokio::time::advance(Duration::from_millis(2)).await;
    wait_for_inputs(&client, before + 1).await;
    let workspace = format!(
        "Workspace registered: {}\nPath: {}\nDescription: project",
        new.workspace_id, new.canonical_path
    );
    let common = format!(
        "Source: System Notification\n{workspace}\n\nAgent Profile created: reviewer\nName: reviewer\nDescription: "
    );
    {
        let inputs = client.inputs.lock().unwrap();
        assert_eq!(inputs[before].session_id, active);
        assert_eq!(
            inputs[before].input,
            format!(
                "{common}\n\nUser reopened todo message {task}: \"One {}\"",
                "字".repeat(76)
            )
        );
    }
    tokio::time::advance(Duration::from_secs(1)).await;
    wait_for_inputs(&client, before + 2).await;
    assert_eq!(client.inputs.lock().unwrap()[before + 1].session_id, idle);
    assert_eq!(client.inputs.lock().unwrap()[before + 1].input, common);
    assert_eq!(
        control.queries().list_turns(&active).await.unwrap().len(),
        1
    );
    assert_eq!(control.queries().list_turns(&idle).await.unwrap().len(), 2);
    assert!(
        service
            .records()
            .get(&never.bot_id)
            .await
            .unwrap()
            .current_session_id
            .is_none()
    );
    let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(sessions, 3);
    stop.send(true).unwrap();
    notices.await.unwrap();
    manual_clock.abort();
}

#[tokio::test]
async fn bot_registrations_exclude_the_initiator_and_failed_delivery_is_forgotten() {
    let (root, pool, client, control, service) = fixture().await;
    let (_, origin) = bot(&service).await;
    let (_, peer) = bot(&service).await;
    let (stop, receiver) = tokio::sync::watch::channel(false);
    let notices = tokio::spawn(service.system_notices().run(receiver));
    let commands =
        WorkspaceCommands::new(workspaces(root.path(), &control), Arc::new(service.clone()));
    *client.blocked.lock().unwrap() = Some(peer.clone());
    let before = client.inputs.lock().unwrap().len();
    commands
        .execute(WorkspaceRequest {
            session_id: origin.clone(),
            command: WorkspaceCommand::Register(register("")),
        })
        .await
        .unwrap();
    wait_for_inputs(&client, before + 1).await;
    assert_eq!(client.inputs.lock().unwrap()[before].session_id, peer);
    *client.blocked.lock().unwrap() = None;
    control
        .agent_profiles()
        .create(profile("second", MODEL, None))
        .await
        .unwrap();
    control.agent_profiles().delete("second").await.unwrap();
    wait_for_inputs(&client, before + 3).await;
    stop.send(true).unwrap();
    notices.await.unwrap();
    for input in &client.inputs.lock().unwrap()[before + 1..] {
        assert_eq!(
            input.input,
            "Source: System Notification\nAgent Profile deleted: second"
        );
    }
    let persisted: i64 = sqlx::query_scalar("SELECT count(*) FROM bot_notifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(persisted, 0);
}
