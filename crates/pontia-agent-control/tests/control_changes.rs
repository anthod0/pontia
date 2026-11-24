use pontia_agent_control::{
    AgentControl, ControlChange, UpsertAgentProfileRequest,
    workspaces::{RegisterWorkspaceRequest, WorkspaceService},
};
use pontia_storage_sqlite::{
    connect_sqlite, repositories::workspaces::SqliteWorkspaceRepository, run_migrations,
};
use tokio::sync::broadcast::error::TryRecvError;

#[tokio::test]
async fn workspace_registration_publishes_committed_insertions_and_reactivations() {
    let root = tempfile::tempdir().unwrap();
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let control = AgentControl::builder(pool.clone(), root.path().into()).build();
    let mut events = control.control_changes().subscribe();
    let service = WorkspaceService::new(
        pool.clone(),
        pontia_config::WorkspaceBrowserConfig {
            roots: vec![pontia_config::WorkspaceRootConfig {
                root_id: "root".into(),
                label: "Test".into(),
                path: root.path().display().to_string(),
            }],
        },
        control.control_changes(),
    );
    let request = RegisterWorkspaceRequest {
        root_id: "root".into(),
        path: String::new(),
        name: Some("Project".into()),
        description: Some("First".into()),
    };
    for first in [true, false] {
        let source = first.then_some("session");
        let workspace = service.register(request.clone(), source).await.unwrap();
        assert_eq!(
            events.try_recv().unwrap(),
            ControlChange::WorkspaceRegistered {
                workspace_id: workspace.workspace_id.clone(),
                name: workspace.name,
                canonical_path: workspace.canonical_path,
                description: Some("First".into()),
                source_session_id: source.map(Into::into)
            }
        );
        assert_eq!(
            control
                .queries()
                .get_workspace(&workspace.workspace_id)
                .await
                .unwrap()
                .unwrap()
                .description
                .as_deref(),
            Some("First")
        );
        service.register(request.clone(), None).await.unwrap();
        assert_eq!(events.try_recv(), Err(TryRecvError::Empty));
        if first {
            SqliteWorkspaceRepository::new(pool.clone())
                .mark_deleted(&workspace.workspace_id)
                .await
                .unwrap();
        }
    }
    for invalid in [
        RegisterWorkspaceRequest {
            path: "missing".into(),
            ..request.clone()
        },
        RegisterWorkspaceRequest {
            description: Some("x".repeat(2001)),
            ..request
        },
    ] {
        assert!(service.register(invalid, None).await.is_err());
    }
    assert_eq!(events.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn profiles_publish_successful_changes_to_bot_visible_fields() {
    let pool = connect_sqlite("sqlite::memory:").await.unwrap();
    run_migrations(&pool).await.unwrap();
    let control = AgentControl::builder(pool, Default::default()).build();
    let mut events = control.control_changes().subscribe();
    let service = control.agent_profiles();
    let mut request = UpsertAgentProfileRequest {
        use_yolo: false,
        profile_id: "review".into(),
        name: "Review".into(),
        description: None,
        client_type: None,
        client_key: None,
        model: None,
        reasoning_effort: None,
        system_prompt: None,
    };
    service.create(request.clone(), None).await.unwrap();
    assert_eq!(
        events.try_recv().unwrap(),
        ControlChange::AgentProfileCreated {
            source_session_id: None,
            profile_id: "review".into(),
            name: "Review".into(),
            description: None
        }
    );
    assert!(service.create(request.clone(), None).await.is_err());
    request.name = "New name".into();
    service
        .update("review", request.clone(), None)
        .await
        .unwrap();
    assert_eq!(events.try_recv(), Err(TryRecvError::Empty));
    request.description = Some("Check changes".into());
    service
        .update("review", request.clone(), None)
        .await
        .unwrap();
    assert_eq!(
        events.try_recv().unwrap(),
        ControlChange::AgentProfileUpdated {
            source_session_id: None,
            profile_id: "review".into(),
            description: request.description.clone(),
            client_type: None,
            client_key: None,
            model: None,
            reasoning_effort: None,
        }
    );
    service.delete("review", None).await.unwrap();
    assert_eq!(
        events.try_recv().unwrap(),
        ControlChange::AgentProfileDeleted {
            source_session_id: None,
            profile_id: "review".into()
        }
    );
    assert!(service.delete("review", None).await.is_err());
    assert!(
        service
            .update("review", request.clone(), None)
            .await
            .is_err()
    );
    request.name.clear();
    assert!(service.create(request, None).await.is_err());
    assert_eq!(events.try_recv(), Err(TryRecvError::Empty));
}
