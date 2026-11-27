use super::*;
use pontia_agent_control::ReportedFact;
use pontia_agent_control::workspaces::upsert_canonical_workspace;
use pontia_storage_sqlite::models::sessions::SessionRow;
use pontia_storage_sqlite::repositories::sessions::{
    SessionOverviewOptions, SqliteSessionRepository,
};
use pontia_storage_sqlite::repositories::workspaces::SqliteWorkspaceRepository;

fn project_helper(session: &str, workspace_id: &str, title: Option<&str>) -> HelperRequest {
    HelperRequest {
        session_id: session.into(),
        command: HelperCommand::Create(CreateHelper {
            location: HelperLocation::WorkspaceId(workspace_id.into()),
            profile_id: None,
            title: title.unwrap_or("Project task").into(),
            description: "Project task".into(),
            message: "Start the project task".into(),
        }),
    }
}

fn temporary_helper(session: &str, name: &str) -> HelperRequest {
    serde_json::from_value(json!({
        "session_id": session, "command": {"action": "create", "input": {
            "temporary": name, "title": "Writing a report", "description": format!("Report for {name}"), "message": "Write a report"
        }}
    }))
    .unwrap()
}

#[tokio::test]
async fn temporary_helpers_share_the_task_folder_and_appear_in_the_default_workspace() {
    let (root, pool, client, control, conversation) = fixture().await;
    let (_, main) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation);
    let default = root.path().join("Pontia");
    let task = "2026-11-20_汇率查询";
    let folder = default.join(task);
    let workspace = pontia_agent_control::workspaces::require_active_workspace(
        &pool,
        default.to_str().unwrap(),
    )
    .await
    .unwrap();
    let error = helpers
        .execute(project_helper(&main, &workspace.workspace_id, None))
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        pontia_bot::helpers::HelperError::Control(Error::Domain(_))
    ));
    let handoff = instructions(&control, &main).await;
    assert!(handoff.contains(&format!(
        "- `{}`\n  - Name: Pontia\n  - Type: Temporary",
        workspace.workspace_id
    )));
    assert!(handoff.contains(&format!(
        "- `projects`: `{}`",
        root.path().join("projects").display()
    )));
    let report = folder.join("previous-report.txt");
    for assignment in 0..2 {
        let response = helpers
            .execute(temporary_helper(&main, task))
            .await
            .unwrap();
        let id = response["session_id"].as_str().unwrap();
        assert_launch_mode(&control, &client, id, LaunchMode::Foreground).await;
        assert_eq!(
            client.launch_directories.lock().unwrap()[id],
            default.canonicalize().unwrap().to_str().unwrap()
        );
        assert!(
            recent_project_ids(&pool, &workspace.workspace_id)
                .await
                .iter()
                .any(|session| session == id)
        );
        if assignment == 0 {
            std::fs::write(&report, "initial findings").unwrap();
        } else {
            assert_eq!(
                std::fs::read_to_string(&report).unwrap(),
                "initial findings"
            );
        }
        let inputs = client.inputs.lock().unwrap();
        let input = &inputs
            .iter()
            .find(|input| input.session_id == id)
            .unwrap()
            .input;
        assert!(input.contains(folder.to_str().unwrap()));
        assert!(input.ends_with("Write a report"));
    }
}

#[tokio::test]
async fn ordinary_creation_rejects_invalid_task_names() {
    let (_root, _, _, control, conversation) = fixture().await;
    let (_, main) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation);
    for name in ["", "one/two", ".", "..", "trailing/"] {
        assert!(
            helpers
                .execute(temporary_helper(&main, name))
                .await
                .is_err(),
            "{name:?}"
        );
    }
    assert_eq!(
        control.queries().list_owned_sessions("bot").await.unwrap(),
        [main]
    );
}

#[tokio::test]
async fn project_helpers_follow_workspace_filters_and_accept_steering_on_their_running_turn() {
    let (root, pool, client, control, conversation) = fixture().await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (bot_id, main) = bot(&conversation).await;
    let mut projects = Vec::new();
    for name in ["first", "second"] {
        let path = root.path().join(name);
        std::fs::create_dir(&path).unwrap();
        let (project, _) = upsert_canonical_workspace(&pool, path.to_str().unwrap(), None)
            .await
            .unwrap();
        let created = helpers
            .execute(project_helper(&main, &project.workspace_id, None))
            .await
            .unwrap();
        assert_eq!(created["delivery"]["status"], "accepted");
        projects.push((project, created["session_id"].as_str().unwrap().to_owned()));
    }
    let private_root = root.path().join("bot").join(&bot_id).join("helpers");
    assert!(!private_root.exists());
    let sessions = helpers.list(&bot_id).await.unwrap();

    for (project, id) in &projects {
        assert_launch_mode(&control, &client, id, LaunchMode::Foreground).await;
        assert_eq!(
            client.launch_directories.lock().unwrap().get(id),
            Some(&project.canonical_path)
        );
        let rows = recent_project_ids(&pool, &project.workspace_id).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(&rows[0], id);
        let session = sessions.iter().find(|s| s.session_id == rows[0]).unwrap();
        assert_eq!(session.workspace_id.as_ref(), Some(&project.workspace_id));
        assert_eq!(session.workspace.as_ref(), Some(&project.canonical_path));
        assert_eq!(
            conversation.resolve(id).await.unwrap().role,
            pontia_bot::identity::BotRole::Helper
        );
    }

    let (project, id) = &projects[0];
    let snapshot = control
        .queries()
        .session_snapshot(id)
        .await
        .unwrap()
        .unwrap();
    let turn = control
        .queries()
        .list_turns(id)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .turn_id;
    report(
        &control.event_ingest_service(),
        id,
        &snapshot.runtimes[0].runtime_id,
        &turn,
        EventType::TurnStarted,
    )
    .await;
    let steer = "User follow-up to running helper";
    let sent = control
        .inbox_commands()
        .send(
            id,
            pontia_agent_control::InputPayload::from(steer),
            pontia_agent_control::SendMode::Immediate,
        )
        .await
        .unwrap();
    assert_eq!(sent, pontia_agent_control::SendOutcome::Delivered);
    assert!(
        client
            .inputs
            .lock()
            .unwrap()
            .iter()
            .any(|i| i.session_id == *id && i.input == steer)
    );
    assert_eq!(control.queries().list_turns(id).await.unwrap().len(), 1);

    let reply = helpers
        .execute(HelperRequest {
            session_id: id.clone(),
            command: HelperCommand::Reply {
                message: "Progress report".into(),
            },
        })
        .await
        .unwrap();
    assert_eq!(reply["delivery"]["status"], "accepted");
    let sentinel = Path::new(&project.canonical_path).join("project.txt");
    std::fs::write(&sentinel, "existing project file").unwrap();
    helpers
        .execute(HelperRequest {
            session_id: main,
            command: HelperCommand::Close {
                session_id: id.clone(),
            },
        })
        .await
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(sentinel).unwrap(),
        "existing project file"
    );
}

#[tokio::test]
async fn project_helper_title_names_its_recent_session() {
    let (root, pool, _, control, conversation) = fixture().await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (_, main) = bot(&conversation).await;
    let path = root.path().join("project");
    std::fs::create_dir(&path).unwrap();
    let (project, _) = upsert_canonical_workspace(&pool, path.to_str().unwrap(), None)
        .await
        .unwrap();
    let created = helpers
        .execute(project_helper(
            &main,
            &project.workspace_id,
            Some("核对文件下载"),
        ))
        .await
        .unwrap();
    let id = created["session_id"].as_str().unwrap();

    let snapshot = control
        .queries()
        .session_snapshot(id)
        .await
        .unwrap()
        .unwrap();
    control
        .event_ingest_service()
        .report_fact(ReportedFact {
            session_id: id.into(),
            turn_id: None,
            report_key: None,
            fact_type: EventType::SessionTitleUpdated,
            data: json!({
                "runtime_id": snapshot.runtimes[0].runtime_id,
                "title": "Client generated title",
            }),
        })
        .await
        .unwrap();

    let recent = recent_project_sessions(&pool, &project.workspace_id).await;
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].session_id, id);
    assert_eq!(recent[0].title.as_deref(), Some("核对文件下载"));
}

#[tokio::test]
async fn invalid_workspace_rejects_the_assignment_without_starting_a_private_helper() {
    let (root, pool, _, control, conversation) = fixture().await;
    let (id, main) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation);
    let (workspace, _) = upsert_canonical_workspace(&pool, root.path().to_str().unwrap(), None)
        .await
        .unwrap();
    assert_eq!(
        SqliteWorkspaceRepository::new(pool.clone())
            .mark_deleted(&workspace.workspace_id)
            .await
            .unwrap(),
        1
    );
    let old_path = root.path().join("old-project");
    let new_path = root.path().join("moved-project");
    std::fs::create_dir(&old_path).unwrap();
    let (moved, _) = upsert_canonical_workspace(&pool, old_path.to_str().unwrap(), None)
        .await
        .unwrap();
    std::fs::rename(&old_path, &new_path).unwrap();
    for invalid in [
        "missing",
        workspace.workspace_id.as_str(),
        moved.workspace_id.as_str(),
    ] {
        assert!(
            helpers
                .execute(project_helper(&main, invalid, None))
                .await
                .is_err()
        );
    }
    assert_eq!(
        control.queries().list_owned_sessions("bot").await.unwrap(),
        [main]
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM bot_helpers")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert!(!root.path().join("bot").join(id).join("helpers").exists());
    assert!(!old_path.exists());
    assert!(new_path.is_dir());
}

async fn recent_project_ids(pool: &sqlx::SqlitePool, workspace_id: &str) -> Vec<String> {
    recent_project_sessions(pool, workspace_id)
        .await
        .into_iter()
        .map(|s| s.session_id)
        .collect()
}

async fn recent_project_sessions(pool: &sqlx::SqlitePool, workspace_id: &str) -> Vec<SessionRow> {
    SqliteSessionRepository::new(pool.clone())
        .session_overview(SessionOverviewOptions {
            include_pinned: false,
            include_archived: false,
            include_active: false,
            include_list: true,
            workspace_id: Some(workspace_id.into()),
            cursor: None,
            list_limit: 50,
        })
        .await
        .unwrap()
        .list
        .unwrap()
        .sessions
}

#[tokio::test]
async fn failed_launcher_keeps_project_and_bot_ownership_in_list_and_next_handoff() {
    let (root, pool, client, control, conversation) = fixture().await;
    let (id, main) = bot(&conversation).await;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let (project, _) = upsert_canonical_workspace(&pool, root.path().to_str().unwrap(), None)
        .await
        .unwrap();
    *client.fail_next_launch.lock().unwrap() = true;
    let failure = helpers
        .execute(project_helper(&main, &project.workspace_id, None))
        .await
        .unwrap_err();
    assert!(
        matches!(failure, pontia_bot::helpers::HelperError::Control(Error::CapabilityUnavailable(message)) if message == "injected launch failure")
    );
    let sessions = helpers.list(&id).await.unwrap();
    assert_eq!(sessions.len(), 1);
    let failed = &sessions[0];
    assert_eq!(failed.state, "error");
    assert_eq!(failed.workspace_id.as_ref(), Some(&project.workspace_id));
    assert_eq!(
        recent_project_ids(&pool, &project.workspace_id).await,
        std::slice::from_ref(&failed.session_id)
    );
    let runtime = control
        .queries()
        .session_snapshot(&main)
        .await
        .unwrap()
        .unwrap()
        .runtimes[0]
        .runtime_id
        .clone();
    control
        .session_commands()
        .terminate_session(&main)
        .await
        .unwrap();
    control
        .event_ingest_service()
        .report_client_fact(
            &main,
            Some(&runtime),
            EventType::SessionExited,
            json!({"reason":"test client exited"}),
        )
        .await
        .unwrap();
    conversation
        .send(&id, "Continue after restart")
        .await
        .unwrap();
    let next = conversation
        .records()
        .get(&id)
        .await
        .unwrap()
        .current_session_id
        .unwrap();
    assert_ne!(next, main);
    assert!(instructions(&control, &next).await.contains(&format!(
        "- `{}`\n  - Type: Ordinary helper\n  - State: error\n  - Workspace ID: `{}`",
        failed.session_id, project.workspace_id
    )));
}

async fn assert_temporary_helper_uses_default_workspace(
    fixture: &Fixture,
    main: &str,
) -> pontia_agent_control::workspaces::WorkspaceRecord {
    let (root, pool, client, control, conversation) = fixture;
    let helpers = HelperService::new(control.clone(), conversation.clone());
    let response = helpers
        .execute(temporary_helper(main, "task"))
        .await
        .unwrap();
    assert_eq!(response["delivery"]["status"], "accepted");
    let id = response["session_id"].as_str().unwrap();
    let canonical = root.path().join("Pontia").canonicalize().unwrap();
    let workspace = pontia_agent_control::workspaces::require_active_workspace(
        pool,
        canonical.to_str().unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(workspace.canonical_path, canonical.to_str().unwrap());
    assert_eq!(
        client.launch_directories.lock().unwrap()[id],
        workspace.canonical_path
    );
    assert_eq!(
        recent_project_ids(pool, &workspace.workspace_id).await,
        [id]
    );
    let folder = canonical.join("task");
    assert!(folder.is_dir());
    let inputs = client.inputs.lock().unwrap();
    let input = &inputs
        .iter()
        .find(|input| input.session_id == id)
        .unwrap()
        .input;
    assert!(input.contains(folder.to_str().unwrap()));
    workspace
}

#[tokio::test]
async fn temporary_helper_restores_the_missing_default_directory() {
    let fixture = fixture().await;
    let (root, pool, _, _, conversation) = &fixture;
    let default = root.path().join("Pontia");
    assert!(!default.exists());
    let (_, main) = bot(conversation).await;
    let canonical = default.canonicalize().unwrap();
    let (workspace, _) =
        upsert_canonical_workspace(pool, canonical.to_str().unwrap(), Some("My tasks"))
            .await
            .unwrap();
    std::fs::remove_dir(&default).unwrap();

    let restored = assert_temporary_helper_uses_default_workspace(&fixture, &main).await;
    assert_eq!(restored.workspace_id, workspace.workspace_id);
    assert_eq!(restored.name.as_deref(), Some("My tasks"));
}

#[tokio::test]
async fn temporary_helper_reactivates_the_default_workspace_without_restarting_the_bot() {
    let fixture = fixture().await;
    let (_, pool, _, control, conversation) = &fixture;
    let (_, main) = bot(conversation).await;
    let workspace = control.queries().list_workspaces().await.unwrap().remove(0);
    let repository = SqliteWorkspaceRepository::new(pool.clone());
    assert_eq!(
        repository
            .mark_deleted(&workspace.workspace_id)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        repository
            .get_workspace(&workspace.workspace_id)
            .await
            .unwrap()
            .unwrap()
            .state,
        "deleted"
    );

    let restored = assert_temporary_helper_uses_default_workspace(&fixture, &main).await;
    assert_eq!(restored.workspace_id, workspace.workspace_id);
}

#[tokio::test]
async fn temporary_helper_registers_the_deleted_default_workspace_without_restarting_the_bot() {
    let fixture = fixture().await;
    let (_, pool, _, control, conversation) = &fixture;
    let (_, main) = bot(conversation).await;
    let workspace = control.queries().list_workspaces().await.unwrap().remove(0);
    assert_eq!(
        sqlx::query("DELETE FROM workspaces WHERE workspace_id = ?")
            .bind(&workspace.workspace_id)
            .execute(pool)
            .await
            .unwrap()
            .rows_affected(),
        1
    );
    assert!(
        control
            .queries()
            .list_workspaces()
            .await
            .unwrap()
            .is_empty()
    );

    let restored = assert_temporary_helper_uses_default_workspace(&fixture, &main).await;
    assert_ne!(restored.workspace_id, workspace.workspace_id);
}

#[cfg(unix)]
#[tokio::test]
async fn temporary_helper_recovers_a_symlinked_default_workspace_using_its_canonical_path() {
    let fixture = fixture().await;
    let (root, pool, _, control, conversation) = &fixture;
    let target = root.path().join("tasks");
    std::fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, root.path().join("Pontia")).unwrap();
    let (_, main) = bot(conversation).await;
    let workspace = control.queries().list_workspaces().await.unwrap().remove(0);
    SqliteWorkspaceRepository::new(pool.clone())
        .mark_deleted(&workspace.workspace_id)
        .await
        .unwrap();

    let restored = assert_temporary_helper_uses_default_workspace(&fixture, &main).await;
    assert_eq!(restored.workspace_id, workspace.workspace_id);
    assert_eq!(
        restored.canonical_path,
        target.canonicalize().unwrap().to_str().unwrap()
    );
}

#[test]
fn external_helper_creation_requires_one_writable_location() {
    for invalid in [
        json!({}),
        json!({"workspace_id":"one", "temporary":"two"}),
        json!({"read_only":null}),
    ] {
        let mut input = json!({"message":"task","description":"test","title":"Test task"});
        input
            .as_object_mut()
            .unwrap()
            .extend(invalid.as_object().unwrap().clone());
        assert!(
            serde_json::from_value::<HelperCommand>(json!({"action":"create", "input":input}))
                .is_err()
        );
    }
}
