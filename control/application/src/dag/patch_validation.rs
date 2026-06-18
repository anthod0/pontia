use super::*;

impl DagService {
    pub(super) async fn ensure_profiles_exist(&self, work_items: &[WorkItemDraft]) -> Result<()> {
        for work_item in work_items {
            let exists: Option<i64> = if let Some(version) = &work_item.execution_profile_version {
                sqlx::query_scalar(
                    "SELECT 1 FROM execution_profiles WHERE profile_id = ? AND version = ?",
                )
                .bind(&work_item.execution_profile_id)
                .bind(version)
                .fetch_optional(&self.pool)
                .await?
            } else {
                sqlx::query_scalar("SELECT 1 FROM execution_profiles WHERE profile_id = ? LIMIT 1")
                    .bind(&work_item.execution_profile_id)
                    .fetch_optional(&self.pool)
                    .await?
            };
            if exists.is_none() {
                return Err(Error::Domain(format!(
                    "execution profile {}{} does not exist",
                    work_item.execution_profile_id,
                    work_item
                        .execution_profile_version
                        .as_ref()
                        .map(|version| format!(" version {version}"))
                        .unwrap_or_default()
                )));
            }
        }
        Ok(())
    }

    pub(super) async fn auto_supersede_work_items(
        &self,
        task_id: &str,
        patch: &DagPatch,
    ) -> Result<Vec<String>> {
        match patch.supersede_policy.as_str() {
            "none" | "explicit_only" => return Ok(Vec::new()),
            "direct_downstream" | "reachable_downstream" => {}
            other => {
                return Err(Error::Domain(format!(
                    "unknown patch supersede_policy {other}"
                )));
            }
        }
        let anchor = patch.anchor_work_item_id.as_deref().ok_or_else(|| {
            Error::Domain(format!(
                "patch supersede_policy {} requires anchor_work_item_id",
                patch.supersede_policy
            ))
        })?;

        let snapshot = GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .task_graph(task_id)
            .await?;
        let active_ids: HashSet<String> = snapshot
            .work_items
            .iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| work_item.work_item_id.clone())
            .collect();
        if !active_ids.contains(anchor) {
            return Err(Error::NotFound(format!("work item {anchor}")));
        }

        let mut candidates = HashSet::new();
        let mut frontier = vec![anchor.to_string()];
        while let Some(from_id) = frontier.pop() {
            for edge in snapshot.edges.iter().filter(|edge| {
                edge.edge_type == GraphEdgeKind::DependsOn
                    && edge.from_work_item_id == from_id
                    && active_ids.contains(&edge.to_work_item_id)
            }) {
                if candidates.insert(edge.to_work_item_id.clone())
                    && patch.supersede_policy == "reachable_downstream"
                {
                    frontier.push(edge.to_work_item_id.clone());
                }
            }
            if patch.supersede_policy == "direct_downstream" {
                break;
            }
        }

        let state_rows = sqlx::query(
            "SELECT work_item_id, current_state FROM work_item_runtime_projection WHERE task_id = ?",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await?;
        let states: HashMap<String, String> = state_rows
            .into_iter()
            .map(|row| (row.get("work_item_id"), row.get("current_state")))
            .collect();

        let mut superseded = Vec::new();
        for work_item_id in candidates {
            match states.get(&work_item_id).map(String::as_str) {
                Some("running") => {
                    return Err(Error::StateConflict(format!(
                        "cannot modify running WorkItem {work_item_id}"
                    )));
                }
                Some("completed") | Some("replan_anchor") | Some("superseded") => {}
                _ => superseded.push(work_item_id),
            }
        }
        Ok(superseded)
    }

    pub(super) async fn validate_patch(&self, task_id: &str, patch: &DagPatch) -> Result<()> {
        validate_supersede_policy(&patch.supersede_policy)?;
        if patch.supersede_policy != "explicit_only" && patch.supersede_policy != "none" {
            let anchor = patch.anchor_work_item_id.as_deref().ok_or_else(|| {
                Error::Domain(format!(
                    "patch supersede_policy {} requires anchor_work_item_id",
                    patch.supersede_policy
                ))
            })?;
            ensure_work_item_exists(&self.pool, &self.graph, task_id, anchor).await?;
        }

        let expanded_operations = expand_patch_operations(&patch.operations);
        let snapshot = GraphProjectionService::new(self.pool.clone(), self.graph.clone())
            .task_graph(task_id)
            .await?;
        let active_ids: HashSet<String> = snapshot
            .work_items
            .iter()
            .filter(|work_item| work_item.active)
            .map(|work_item| work_item.work_item_id.clone())
            .collect();
        let active_edge_keys: HashSet<(String, String, String)> = snapshot
            .edges
            .iter()
            .map(|edge| {
                (
                    edge.from_work_item_id.clone(),
                    edge.to_work_item_id.clone(),
                    edge.edge_type.as_str().to_string(),
                )
            })
            .collect();

        let mut added_work_items = Vec::new();
        let mut temp_ids = HashSet::new();
        for operation in &expanded_operations {
            match operation {
                PatchOperation::AddWorkItem { work_item } => {
                    dag_validator::validate_work_item_drafts(std::slice::from_ref(work_item))?;
                    if let Some(temp_id) = &work_item.temp_id
                        && !temp_ids.insert(temp_id.clone())
                    {
                        return Err(Error::Domain(format!(
                            "duplicate patch work item temp_id: {temp_id}"
                        )));
                    }
                    added_work_items.push(work_item.clone());
                }
                PatchOperation::AddEdge { edge } => {
                    dag_validator::validate_edge_type(&edge.edge_type)?;
                }
                PatchOperation::RemoveEdge { edge } => {
                    dag_validator::validate_edge_type(&edge.edge_type)?;
                    if !active_edge_keys.contains(&(
                        edge.from_work_item_id.clone(),
                        edge.to_work_item_id.clone(),
                        edge.edge_type.clone(),
                    )) {
                        return Err(Error::NotFound(format!(
                            "active edge {} -> {}",
                            edge.from_work_item_id, edge.to_work_item_id
                        )));
                    }
                }
                PatchOperation::SupersedeWorkItem { work_item_id, .. } => {
                    ensure_work_item_exists(&self.pool, &self.graph, task_id, work_item_id).await?;
                    ensure_work_item_not_running(&self.pool, task_id, work_item_id).await?;
                }
                PatchOperation::ReactivateWorkItem { work_item_id, .. }
                | PatchOperation::SetWorkItemOutcome { work_item_id, .. } => {
                    let exists = GraphProjectionService::new(self.pool.clone(), self.graph.clone())
                        .get_work_item(work_item_id)
                        .await?
                        .is_some_and(|work_item| work_item.task_id == task_id);
                    if !exists {
                        return Err(Error::NotFound(format!("work item {work_item_id}")));
                    }
                    ensure_work_item_not_running(&self.pool, task_id, work_item_id).await?;
                }
                PatchOperation::ReplaceEdge { .. }
                | PatchOperation::InsertWorkItemBetween { .. }
                | PatchOperation::ReplaceDownstream { .. } => {}
            }
        }
        self.ensure_profiles_exist(&added_work_items).await?;

        let mut superseded: HashSet<String> = self
            .auto_supersede_work_items(task_id, patch)
            .await?
            .into_iter()
            .collect();
        superseded.extend(
            expanded_operations
                .iter()
                .filter_map(|operation| match operation {
                    PatchOperation::SupersedeWorkItem { work_item_id, .. } => {
                        Some(work_item_id.clone())
                    }
                    _ => None,
                }),
        );
        let mut nodes: Vec<String> = snapshot
            .work_items
            .iter()
            .filter(|work_item| work_item.active && !superseded.contains(&work_item.work_item_id))
            .map(|work_item| work_item.work_item_id.clone())
            .collect();
        let mut temp_to_generated = HashMap::new();
        for work_item in &added_work_items {
            let generated = format!("__new_{}", temp_to_generated.len());
            if let Some(temp_id) = &work_item.temp_id {
                temp_to_generated.insert(temp_id.clone(), generated.clone());
            }
            nodes.push(generated);
        }

        let mut edges: Vec<WorkItemEdgeDraft> = snapshot
            .edges
            .iter()
            .filter(|edge| {
                edge.edge_type == GraphEdgeKind::DependsOn
                    && !superseded.contains(&edge.from_work_item_id)
                    && !superseded.contains(&edge.to_work_item_id)
            })
            .map(|edge| WorkItemEdgeDraft {
                from_work_item_id: edge.from_work_item_id.clone(),
                to_work_item_id: edge.to_work_item_id.clone(),
                edge_type: edge.edge_type.as_str().to_string(),
            })
            .collect();
        for operation in &expanded_operations {
            match operation {
                PatchOperation::RemoveEdge { edge } => {
                    edges.retain(|existing| {
                        !(existing.from_work_item_id == edge.from_work_item_id
                            && existing.to_work_item_id == edge.to_work_item_id
                            && existing.edge_type == edge.edge_type)
                    });
                }
                PatchOperation::AddEdge { edge } => {
                    let from = resolve_patch_ref(
                        &edge.from_work_item_id,
                        &temp_to_generated,
                        &nodes,
                        "from",
                    )?;
                    let to =
                        resolve_patch_ref(&edge.to_work_item_id, &temp_to_generated, &nodes, "to")?;
                    if active_ids.contains(&from) {
                        ensure_work_item_not_running(&self.pool, task_id, &from).await?;
                    }
                    if active_ids.contains(&to) {
                        ensure_work_item_not_running(&self.pool, task_id, &to).await?;
                    }
                    edges.push(WorkItemEdgeDraft {
                        from_work_item_id: from,
                        to_work_item_id: to,
                        edge_type: edge.edge_type.clone(),
                    });
                }
                _ => {}
            }
        }
        dag_validator::validate_acyclic(nodes, &edges)
    }
}
