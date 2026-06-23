use super::*;
use pontia_storage_sqlite::repositories::dag::SqliteDagRepository;

#[derive(Clone)]
pub struct DagQueryService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
}

impl DagQueryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_graph(pool, GraphRuntimeConfig::default())
    }

    pub fn with_graph(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self { pool, graph }
    }

    pub async fn get_task(&self, task_id: &str) -> Result<Option<TaskView>> {
        ExternalQueryService::new(self.pool.clone())
            .get_task(task_id)
            .await
    }

    pub async fn list_relevant_dag_proposals(&self, task_id: &str) -> Result<Vec<DagProposal>> {
        let rows = SqliteDagRepository::new(self.pool.clone())
            .list_relevant_dag_proposals(task_id)
            .await?;
        rows.into_iter().map(dag_proposal_row_to_record).collect()
    }

    pub async fn get_task_dag(&self, task_id: &str) -> Result<TaskDagView> {
        let summary = self.get_task_dag_summary(task_id).await?;
        let work_items = self.list_work_items(task_id).await?;
        let edges = self.list_work_item_edges(task_id).await?;
        let runs = self.list_work_item_runs(task_id).await?;
        let signals = self.list_dag_signals(task_id).await?;
        Ok(TaskDagView {
            task_id: task_id.to_string(),
            summary,
            work_items,
            edges,
            runs,
            signals,
        })
    }

    pub async fn get_task_dag_summary(&self, task_id: &str) -> Result<DagSummaryView> {
        let graph = self.task_graph_snapshot(task_id).await?;
        let runtime = self.runtime_map(task_id).await?;
        let active_ids: std::collections::HashSet<_> = graph
            .work_items
            .iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| work_item.work_item_id.as_str())
            .collect();

        let mut summary = DagSummaryView {
            total_work_items: active_ids.len() as i64,
            ready_work_items: 0,
            running_work_items: 0,
            completed_work_items: 0,
            blocked_work_items: 0,
            failed_work_items: 0,
            open_signals: 0,
            total_runs: 0,
        };

        for (work_item_id, runtime) in &runtime {
            if !active_ids.contains(work_item_id.as_str()) {
                continue;
            }
            match runtime.current_state.as_str() {
                "ready" => summary.ready_work_items += 1,
                "running" => summary.running_work_items += 1,
                "completed" => summary.completed_work_items += 1,
                "blocked" | "needs_input" => summary.blocked_work_items += 1,
                "failed" => summary.failed_work_items += 1,
                _ => {}
            }
        }

        summary.open_signals = self.count_open_signals(task_id).await?;
        summary.total_runs = self.count_work_item_runs(task_id).await?;
        Ok(summary)
    }

    pub async fn list_work_items(&self, task_id: &str) -> Result<Vec<WorkItemWithRuntimeView>> {
        let graph = self.task_graph_snapshot(task_id).await?;
        let runtime = self.runtime_map(task_id).await?;
        Ok(graph
            .work_items
            .into_iter()
            .map(|node| {
                let runtime = runtime.get(&node.work_item_id).cloned();
                WorkItemWithRuntimeView {
                    work_item: work_item_node_to_record(node),
                    runtime,
                }
            })
            .collect())
    }

    pub async fn list_work_item_edges(&self, task_id: &str) -> Result<Vec<WorkItemEdgeView>> {
        Ok(self
            .task_graph_snapshot(task_id)
            .await?
            .edges
            .into_iter()
            .map(graph_edge_record_to_view)
            .collect())
    }

    pub async fn list_work_item_runs(&self, task_id: &str) -> Result<Vec<WorkItemRunRecord>> {
        let repository = SqliteDagRepository::new(self.pool.clone());
        let rows = repository.list_work_item_runs(task_id).await?;

        rows.into_iter().map(work_item_run_row_to_record).collect()
    }

    pub async fn list_dag_signals(&self, task_id: &str) -> Result<Vec<DagSignalRecord>> {
        let repository = SqliteDagRepository::new(self.pool.clone());
        let rows = repository.list_dag_signals(task_id).await?;

        rows.into_iter().map(dag_signal_row_to_record).collect()
    }

    async fn runtime_map(
        &self,
        task_id: &str,
    ) -> Result<std::collections::HashMap<String, WorkItemRuntimeView>> {
        let repository = SqliteDagRepository::new(self.pool.clone());
        let rows = repository.list_runtime_projection(task_id).await?;

        let mut runtime = std::collections::HashMap::new();
        for row in rows {
            runtime.insert(row.work_item_id.clone(), work_item_runtime_row_to_view(row));
        }
        Ok(runtime)
    }

    async fn count_open_signals(&self, task_id: &str) -> Result<i64> {
        SqliteDagRepository::new(self.pool.clone())
            .count_open_signals(task_id)
            .await
    }

    async fn count_work_item_runs(&self, task_id: &str) -> Result<i64> {
        SqliteDagRepository::new(self.pool.clone())
            .count_work_item_runs(task_id)
            .await
    }

    async fn task_graph_snapshot(&self, task_id: &str) -> Result<TaskGraphSnapshot> {
        GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .task_graph(task_id)
            .await
    }
}
