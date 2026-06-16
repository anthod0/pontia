#[cfg(feature = "lbug")]
use pontia::application::LbugDagGraphStore;
#[cfg(feature = "lbug")]
use pontia::application::{
    AddWorkItemEdgeRequest, GraphEdgeKind, GraphProjectionService, GraphRuntimeConfig,
    UpsertTaskRequest, UpsertWorkItemRequest,
};
use pontia::storage::sqlite::{connect_sqlite, run_migrations};
#[cfg(feature = "lbug")]
use serde_json::json;
use sqlx::SqlitePool;

#[cfg(feature = "lbug")]
fn task_request(task_id: &str) -> UpsertTaskRequest {
    UpsertTaskRequest {
        task_id: task_id.to_string(),
        title: "Graph task".to_string(),
        description: "Persist graph data".to_string(),
        ref_: Some("event:task.created:evt_1".to_string()),
        metadata: json!({"source":"test"}),
    }
}

#[cfg(feature = "lbug")]
fn work_item_request(
    task_id: &str,
    work_item_id: &str,
    title: &str,
    priority: i64,
) -> UpsertWorkItemRequest {
    UpsertWorkItemRequest {
        work_item_id: work_item_id.to_string(),
        task_id: task_id.to_string(),
        title: title.to_string(),
        description: format!("Work item {title}"),
        kind: "implementation".to_string(),
        action: "agent_turn".to_string(),
        execution_profile_id: "default".to_string(),
        execution_profile_version: None,
        review_policy: None,
        execution_policy: None,
        escalation_policy: None,
        priority,
        optional: false,
        parallelizable: true,
        acceptance_criteria: json!(["done"]),
        active: true,
        ref_: None,
        metadata: json!({}),
    }
}

async fn test_pool() -> SqlitePool {
    let db = connect_sqlite("sqlite::memory:")
        .await
        .expect("connect sqlite");
    run_migrations(&db).await.expect("migrate");
    db
}

#[cfg(feature = "lbug")]
async fn insert_task(pool: &SqlitePool, task_id: &str) {
    sqlx::query(
        r#"INSERT INTO tasks (task_id, state, input, routing_state)
           VALUES (?, 'running', 'Graph projection test task', 'resolved')"#,
    )
    .bind(task_id)
    .execute(pool)
    .await
    .expect("insert task");
}

#[tokio::test]
async fn migrations_do_not_create_sqlite_graph_store_tables() {
    let pool = test_pool().await;
    let table_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM sqlite_master
           WHERE type = 'table' AND name IN (
             'graph_tasks', 'graph_work_items', 'graph_work_item_edges', 'graph_signals'
           )"#,
    )
    .fetch_one(&pool)
    .await
    .expect("count graph tables");
    assert_eq!(table_count, 0);
}

#[cfg(feature = "lbug")]
#[tokio::test]
async fn lbug_graph_store_creates_missing_parent_directory() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let db_path = temp_dir.path().join("missing-parent").join("graphdb");

    LbugDagGraphStore::open(&db_path)
        .await
        .expect("open lbug graph store with missing parent");

    assert!(
        db_path.parent().expect("db path parent").exists(),
        "graph store should create the database parent directory"
    );
}

#[cfg(feature = "lbug")]
#[tokio::test]
async fn lbug_graph_store_persists_work_items_edges_and_active_state() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let store = LbugDagGraphStore::open(temp_dir.path().join("graphdb"))
        .await
        .expect("open lbug graph store");

    store
        .upsert_task(task_request("task_lbug_graph_store"))
        .await
        .expect("upsert task");
    for (work_item_id, title, priority) in [("wi_a", "A", 10), ("wi_b", "B", 0)] {
        store
            .upsert_work_item(work_item_request(
                "task_lbug_graph_store",
                work_item_id,
                title,
                priority,
            ))
            .await
            .expect("upsert work item");
    }
    let edge = AddWorkItemEdgeRequest {
        task_id: "task_lbug_graph_store".to_string(),
        from_work_item_id: "wi_a".to_string(),
        to_work_item_id: "wi_b".to_string(),
        edge_type: GraphEdgeKind::DependsOn,
        ref_: None,
    };
    store.add_edge(edge.clone()).await.expect("add edge");
    store.add_edge(edge).await.expect("idempotent edge");
    store
        .set_work_item_active("wi_a", false)
        .await
        .expect("deactivate work item");

    let snapshot = store
        .task_graph("task_lbug_graph_store")
        .await
        .expect("snapshot");
    assert_eq!(snapshot.task.as_ref().unwrap().title, "Graph task");
    assert_eq!(snapshot.work_items.len(), 2);
    assert_eq!(snapshot.edges.len(), 1);
    assert_eq!(snapshot.edges[0].edge_type, GraphEdgeKind::DependsOn);
    assert!(
        !snapshot
            .work_items
            .iter()
            .find(|work_item| work_item.work_item_id == "wi_a")
            .unwrap()
            .active
    );

    let dependencies = store.list_dependencies("wi_b").await.expect("dependencies");
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].work_item_id, "wi_a");
}

#[cfg(feature = "lbug")]
#[tokio::test]
async fn graph_projection_projects_events_to_lbug_when_enabled() {
    let pool = test_pool().await;
    insert_task(&pool, "task_lbug_projection").await;
    sqlx::query(
        r#"INSERT INTO task_events (event_id, task_id, event_type, payload)
           VALUES
             ('evt_task_created_lbug', 'task_lbug_projection', 'task.created', ?),
             ('evt_wi_lbug', 'task_lbug_projection', 'work_item.created', ?)"#,
    )
    .bind(json!({"input":"Build a lbug-backed DAG"}).to_string())
    .bind(
        json!({
            "work_item_id":"wi_lbug_design",
            "task_id":"task_lbug_projection",
            "title":"Design",
            "description":"Design graph model",
            "kind":"design",
            "action":"agent_turn",
            "execution_profile_id":"planner",
            "priority": 5
        })
        .to_string(),
    )
    .execute(&pool)
    .await
    .expect("insert task events");
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let graph = GraphRuntimeConfig {
        enabled: true,
        db_dir: Some(temp_dir.path().join("graphdb").display().to_string()),
    };

    GraphProjectionService::new(pool, graph.clone())
        .project_task("task_lbug_projection")
        .await
        .expect("project task");

    let snapshot = LbugDagGraphStore::open(graph.db_dir.unwrap())
        .await
        .expect("open lbug store")
        .task_graph("task_lbug_projection")
        .await
        .expect("snapshot");
    assert_eq!(
        snapshot.task.as_ref().unwrap().title,
        "Build a lbug-backed DAG"
    );
    assert_eq!(snapshot.work_items.len(), 1);
    assert_eq!(snapshot.work_items[0].work_item_id, "wi_lbug_design");
}

#[cfg(feature = "lbug")]
#[tokio::test]
async fn graph_projection_rebuilds_from_task_events() {
    let pool = test_pool().await;
    insert_task(&pool, "task_graph_projection").await;

    for (event_id, event_type, payload) in [
        (
            "evt_task_created",
            "task.created",
            json!({"input":"Build a graph-backed DAG"}),
        ),
        (
            "evt_dag_applied",
            "dag.applied",
            json!({"proposal_id":"dagprop_1"}),
        ),
        (
            "evt_wi_a",
            "work_item.created",
            json!({
                "work_item_id":"wi_design",
                "task_id":"task_graph_projection",
                "title":"Design",
                "description":"Design graph model",
                "kind":"design",
                "action":"agent_turn",
                "execution_profile_id":"planner",
                "execution_profile_version": null,
                "priority": 5,
                "optional": false,
                "parallelizable": true,
                "acceptance_criteria":["design recorded"],
                "metadata":{"phase":"design"}
            }),
        ),
        (
            "evt_wi_b",
            "work_item.created",
            json!({
                "work_item_id":"wi_impl",
                "task_id":"task_graph_projection",
                "title":"Implement",
                "description":"Implement graph store",
                "kind":"implementation",
                "action":"agent_turn",
                "execution_profile_id":"implementer",
                "priority": 1
            }),
        ),
        (
            "evt_edge",
            "work_item.edge_added",
            json!({
                "task_id":"task_graph_projection",
                "from_work_item_id":"wi_design",
                "to_work_item_id":"wi_impl",
                "edge_type":"depends_on"
            }),
        ),
        (
            "evt_signal",
            "signal.emitted",
            json!({
                "signal_id":"sig_1",
                "task_id":"task_graph_projection",
                "work_item_id":"wi_impl",
                "source":"agent",
                "kind":"risk",
                "summary":"Migration risk",
                "detail":"Schema is destructive",
                "severity":"high",
                "related_refs":["work_item:wi_impl"]
            }),
        ),
    ] {
        sqlx::query(
            r#"INSERT INTO task_events (event_id, task_id, event_type, payload)
               VALUES (?, 'task_graph_projection', ?, ?)"#,
        )
        .bind(event_id)
        .bind(event_type)
        .bind(payload.to_string())
        .execute(&pool)
        .await
        .expect("insert task event");
    }

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let graph = GraphRuntimeConfig {
        enabled: true,
        db_dir: Some(temp_dir.path().join("graphdb").display().to_string()),
    };
    let projection = GraphProjectionService::new(pool.clone(), graph.clone());
    projection
        .project_task("task_graph_projection")
        .await
        .expect("project task");
    projection
        .project_task("task_graph_projection")
        .await
        .expect("idempotent replay");

    let snapshot = LbugDagGraphStore::open(graph.db_dir.unwrap())
        .await
        .expect("open lbug store")
        .task_graph("task_graph_projection")
        .await
        .expect("snapshot");
    assert_eq!(
        snapshot.task.as_ref().unwrap().title,
        "Build a graph-backed DAG"
    );
    assert_eq!(snapshot.work_items.len(), 2);
    assert_eq!(snapshot.edges.len(), 1);
    assert_eq!(snapshot.signals.len(), 1);
    assert_eq!(snapshot.signals[0].severity, "high");
}
