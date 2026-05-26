use super::*;

mod input;
mod rendering;
mod resolver;
mod types;

use input::{
    default_agent_tool_input, is_known_tool, parse_planning_role, parse_submit_plan_initial_input,
    parse_submit_plan_patch_input, reject_duplicate_successful_submit_plan, validate_required,
};
use rendering::{render_execution_context, render_planning_context};
pub use resolver::AgentToolContextResolver;
pub use types::{
    AgentPlanningRole, AgentToolContext, AgentToolMode, AgentToolRequest, AgentToolResponse,
    GetContextToolResponse, RaiseSignalToolResponse, SubmitPlanToolResponse,
    SubmitResultToolResponse,
};

pub struct AgentToolService {
    pool: SqlitePool,
    graph: GraphRuntimeConfig,
    resolver: AgentToolContextResolver,
    queries: ExternalQueryService,
    profiles: AgentProfileService,
}

impl AgentToolService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_graph(pool, GraphRuntimeConfig::default())
    }

    pub fn with_graph(pool: SqlitePool, graph: GraphRuntimeConfig) -> Self {
        Self {
            pool: pool.clone(),
            graph: graph.clone(),
            resolver: AgentToolContextResolver::new(pool.clone()),
            queries: ExternalQueryService::with_graph(pool.clone(), graph),
            profiles: AgentProfileService::new(pool),
        }
    }

    pub async fn call(
        &self,
        tool_name: &str,
        request: AgentToolRequest,
    ) -> Result<AgentToolResponse> {
        if !is_known_tool(tool_name) {
            return Err(Error::NotFound(format!("agent tool {tool_name} not found")));
        }
        let context = self.resolver.resolve(&request).await?;
        match tool_name {
            "getContext" => self.get_context(context).await,
            "submitPlan" => self.submit_plan(context, request.input).await,
            "submitResult" => self.submit_result(context, request.input).await,
            "raiseSignal" => self.raise_signal(context, request.input).await,
            _ => Ok(AgentToolResponse::Skeleton { context }),
        }
    }

    async fn submit_result(
        &self,
        context: AgentToolContext,
        input: Value,
    ) -> Result<AgentToolResponse> {
        if !matches!(&context.mode, AgentToolMode::Execution { .. }) {
            return Err(Error::StateConflict(
                "submitResult requires a DAG execution turn".to_string(),
            ));
        }
        let payload: SubmitResultPayload = serde_json::from_value(input)
            .map_err(|err| Error::Domain(format!("invalid submitResult input: {err}")))?;
        let outcome = DagRunResultService::with_graph(self.pool.clone(), self.graph.clone())
            .submit_tool_result(&context, payload)
            .await?;
        Ok(AgentToolResponse::SubmitResult(SubmitResultToolResponse {
            task_id: outcome.task_id,
            work_item_id: outcome.work_item_id,
            run_id: outcome.run_id,
            state: outcome.state,
            scheduler: outcome.scheduler,
        }))
    }

    async fn raise_signal(
        &self,
        context: AgentToolContext,
        input: Value,
    ) -> Result<AgentToolResponse> {
        let payload: RaiseSignalPayload = serde_json::from_value(input)
            .map_err(|err| Error::Domain(format!("invalid raiseSignal input: {err}")))?;
        let outcome = DagRunResultService::with_graph(self.pool.clone(), self.graph.clone())
            .raise_tool_signal(&context, payload)
            .await?;
        Ok(AgentToolResponse::RaiseSignal(RaiseSignalToolResponse {
            signal_id: outcome.signal_id,
            task_id: outcome.task_id,
            work_item_id: outcome.work_item_id,
            run_id: outcome.run_id,
            kind: outcome.kind,
            state: outcome.state,
            policy: json!({ "replanner_started": outcome.replanner_started }),
        }))
    }

    async fn submit_plan(
        &self,
        context: AgentToolContext,
        input: Value,
    ) -> Result<AgentToolResponse> {
        let AgentToolMode::Planning { role } = &context.mode else {
            return Err(Error::StateConflict(
                "submitPlan requires a DAG planning turn".to_string(),
            ));
        };

        reject_duplicate_successful_submit_plan(&self.pool, &context).await?;

        let mode = input
            .get("mode")
            .and_then(Value::as_str)
            .ok_or_else(|| Error::Domain("submitPlan input missing mode".to_string()))?;
        let planning = DagPlanningService::new(self.pool.clone());
        let outcome = match (role, mode) {
            (AgentPlanningRole::Planner, "initial_dag") => {
                let payload = parse_submit_plan_initial_input(input)?;
                planning
                    .submit_initial_plan_payload(&context.task_id, &context.session_id, payload)
                    .await?
            }
            (AgentPlanningRole::Planner, "patch") => {
                return Err(Error::StateConflict(
                    "Planner can only submit initial_dag plans".to_string(),
                ));
            }
            (AgentPlanningRole::Replanner, "patch") => {
                let (summary, patch) = parse_submit_plan_patch_input(input)?;
                planning
                    .submit_patch_payload(&context.task_id, &context.session_id, summary, patch)
                    .await?
            }
            (AgentPlanningRole::Replanner, "initial_dag") => {
                return Err(Error::StateConflict(
                    "RePlanner can only submit patch plans".to_string(),
                ));
            }
            (_, other) => {
                return Err(Error::Domain(format!(
                    "submitPlan mode must be initial_dag or patch, got {other}"
                )));
            }
        };

        Ok(AgentToolResponse::SubmitPlan(SubmitPlanToolResponse {
            proposal_id: outcome.proposal.proposal_id.clone(),
            validation: json!({"ok": true}),
            apply: json!({
                "applied": true,
                "proposal_state": outcome.proposal.state,
                "mode": outcome.proposal.mode,
            }),
            scheduler: outcome.scheduler,
        }))
    }

    async fn get_context(&self, context: AgentToolContext) -> Result<AgentToolResponse> {
        let task = self
            .queries
            .get_task(&context.task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {} not found", context.task_id)))?;

        match context.mode.clone() {
            AgentToolMode::Planning { role } => {
                let dag = self.queries.get_task_dag(&context.task_id).await?;
                let open_signals: Vec<_> = dag
                    .signals
                    .iter()
                    .filter(|signal| signal.state == "open")
                    .cloned()
                    .collect();
                let relevant_proposals = self
                    .queries
                    .list_relevant_dag_proposals(&context.task_id)
                    .await?;
                let execution_profiles = self.profiles.list_latest().await?;

                Ok(AgentToolResponse::GetContext(GetContextToolResponse {
                    text: render_planning_context(
                        role,
                        &task,
                        &dag,
                        &open_signals,
                        &relevant_proposals,
                        &execution_profiles,
                    ),
                }))
            }
            AgentToolMode::Execution {
                run_id,
                work_item_id,
            } => {
                let work_items = self.queries.list_work_items(&context.task_id).await?;
                let work_item = work_items
                    .iter()
                    .find(|item| item.work_item.work_item_id == *work_item_id)
                    .cloned()
                    .ok_or_else(|| {
                        Error::NotFound(format!("work item {work_item_id} not found"))
                    })?;
                let work_item_run = self
                    .queries
                    .list_work_item_runs(&context.task_id)
                    .await?
                    .into_iter()
                    .find(|run| run.run_id == *run_id)
                    .ok_or_else(|| Error::NotFound(format!("work item run {run_id} not found")))?;
                let edges = self.queries.list_work_item_edges(&context.task_id).await?;
                let dependencies: Vec<_> = edges
                    .into_iter()
                    .filter(|edge| edge.to_work_item_id == *work_item_id)
                    .collect();
                let upstream_completed_items: Vec<_> = dependencies
                    .iter()
                    .filter_map(|edge| {
                        work_items.iter().find(|item| {
                            item.work_item.work_item_id == edge.from_work_item_id
                                && item
                                    .runtime
                                    .as_ref()
                                    .map(|runtime| runtime.current_state.as_str())
                                    == Some("completed")
                        })
                    })
                    .cloned()
                    .collect();
                let open_signals: Vec<_> = self
                    .queries
                    .list_dag_signals(&context.task_id)
                    .await?
                    .into_iter()
                    .filter(|signal| {
                        signal.state == "open"
                            && (signal.work_item_id.as_deref().is_none()
                                || signal.work_item_id.as_deref() == Some(work_item_id.as_str())
                                || signal.run_id.as_deref() == Some(run_id.as_str()))
                    })
                    .collect();
                let acceptance_criteria = work_item.work_item.acceptance_criteria.clone();

                Ok(AgentToolResponse::GetContext(GetContextToolResponse {
                    text: render_execution_context(
                        &task,
                        &work_item,
                        &work_item_run,
                        &upstream_completed_items,
                        &acceptance_criteria,
                        &open_signals,
                    ),
                }))
            }
        }
    }
}
