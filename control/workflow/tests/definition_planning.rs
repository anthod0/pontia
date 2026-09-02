use pontia_workflow::{
    AcceptedWorkflowDefinition, AcceptedWorkflowNode, DefinitionChangePlan,
    WorkflowDefinitionHandoff, WorkflowNodeDefinition, plan_workflow_definition_change,
};

fn node(
    id: &str,
    parent_node_id: Option<&str>,
    title: &str,
    input: &str,
    output: &str,
) -> AcceptedWorkflowNode {
    AcceptedWorkflowNode {
        node_id: id.to_string(),
        parent_node_id: parent_node_id.map(str::to_string),
        activated: id == "node_current",
        definition: WorkflowNodeDefinition {
            node_type: "agent".to_string(),
            phase: "Build".to_string(),
            title: title.to_string(),
            instructions: format!("Do {title}."),
            inputs: if input.is_empty() {
                Vec::new()
            } else {
                vec![input.to_string()]
            },
            output: output.to_string(),
            execution_profile_id: None,
            execution_profile_version: None,
        },
    }
}

fn accepted_definition() -> AcceptedWorkflowDefinition {
    AcceptedWorkflowDefinition {
        workflow_id: "wf_plan".to_string(),
        revision: 2,
        title: "Plan workflow".to_string(),
        cwd: "/workspace/project".to_string(),
        handoffs: vec![WorkflowDefinitionHandoff {
            name: "brief.md".to_string(),
            source: "handoff/brief.md".to_string(),
        }],
        nodes: vec![
            node("node_current", None, "Current", "brief.md", "current.md"),
            node(
                "node_future_one",
                Some("node_current"),
                "Future one",
                "current.md",
                "one.md",
            ),
            node(
                "node_future_two",
                Some("node_future_one"),
                "Future two",
                "one.md",
                "two.md",
            ),
        ],
        retired_node_ids: vec!["node_retired".to_string()],
    }
}

fn candidate_with_nodes(nodes: &str) -> Vec<u8> {
    format!(
        r#"workflow_id = "wf_plan"
revision = 2
title = "Plan workflow"
cwd = "/workspace/project"

[[handoffs]]
name = "brief.md"
source = "handoff/brief.md"

{nodes}"#
    )
    .into_bytes()
}

fn candidate_node(id: Option<&str>, title: &str, input: &str, output: &str) -> String {
    let id = id.map_or_else(String::new, |id| format!("id = {id:?}\n"));
    let inputs = if input.is_empty() {
        "[]".to_string()
    } else {
        format!("[{input:?}]")
    };
    format!(
        r#"[[nodes]]
{id}type = "agent"
phase = "Build"
title = {title:?}
instructions = {instructions:?}
inputs = {inputs}
output = {output:?}
"#,
        instructions = format!("Do {title}.")
    )
}

#[test]
fn formatting_and_nonsemantic_order_changes_are_no_change() {
    let candidate = br#"
revision = 2
workflow_id = "wf_plan"
cwd = "/workspace/project"
title = "Plan workflow"

[[nodes]]
output = "current.md"
inputs = ["brief.md"]
instructions = "Do Current."
title = "Current"
phase = "Build"
type = "agent"
id = "node_current"

[[nodes]]
id = "node_future_one"
type = "agent"
phase = "Build"
title = "Future one"
instructions = "Do Future one."
inputs = ["current.md"]
output = "one.md"

[[nodes]]
id = "node_future_two"
type = "agent"
phase = "Build"
title = "Future two"
instructions = "Do Future two."
inputs = ["one.md"]
output = "two.md"

[[handoffs]]
source = "handoff/brief.md"
name = "brief.md"
"#;

    assert_eq!(
        plan_workflow_definition_change(&accepted_definition(), candidate).expect("plan"),
        DefinitionChangePlan::NoChange
    );
}

#[test]
fn handoff_and_input_order_do_not_change_semantics() {
    let mut accepted = accepted_definition();
    accepted.handoffs.push(WorkflowDefinitionHandoff {
        name: "style.md".to_string(),
        source: "handoff/style.md".to_string(),
    });
    accepted.nodes[0].definition.inputs = vec!["brief.md".to_string(), "style.md".to_string()];
    let candidate = br#"workflow_id = "wf_plan"
revision = 2
title = "Plan workflow"
cwd = "/workspace/project"

[[handoffs]]
name = "style.md"
source = "handoff/style.md"

[[handoffs]]
name = "brief.md"
source = "handoff/brief.md"

[[nodes]]
id = "node_current"
type = "agent"
phase = "Build"
title = "Current"
instructions = "Do Current."
inputs = ["style.md", "brief.md"]
output = "current.md"

[[nodes]]
id = "node_future_one"
type = "agent"
phase = "Build"
title = "Future one"
instructions = "Do Future one."
inputs = ["current.md"]
output = "one.md"

[[nodes]]
id = "node_future_two"
type = "agent"
phase = "Build"
title = "Future two"
instructions = "Do Future two."
inputs = ["one.md"]
output = "two.md"
"#;

    assert_eq!(
        plan_workflow_definition_change(&accepted, candidate).expect("plan"),
        DefinitionChangePlan::NoChange
    );
}

#[test]
fn changed_node_replaces_the_affected_downstream_suffix() {
    let nodes = [
        candidate_node(Some("node_current"), "Current", "brief.md", "current.md"),
        candidate_node(None, "Replacement one", "current.md", "one.md"),
        candidate_node(None, "Future two", "one.md", "two.md"),
    ]
    .join("\n");

    let plan =
        plan_workflow_definition_change(&accepted_definition(), &candidate_with_nodes(&nodes))
            .expect("plan replacement");

    let DefinitionChangePlan::Changed {
        retained_node_ids,
        retired_node_ids,
        introduced_nodes,
    } = plan
    else {
        panic!("expected changed plan");
    };
    assert_eq!(retained_node_ids, ["node_current"]);
    assert_eq!(retired_node_ids, ["node_future_one", "node_future_two"]);
    assert_eq!(introduced_nodes.len(), 2);
    assert_eq!(
        introduced_nodes[0].parent,
        Some(pontia_workflow::PlannedNodeParent::Retained(
            "node_current".to_string()
        ))
    );
    assert_eq!(
        introduced_nodes[1].parent,
        Some(pontia_workflow::PlannedNodeParent::Introduced(0))
    );
    assert_eq!(introduced_nodes[0].definition.title, "Replacement one");
}

#[test]
fn unchanged_future_nodes_are_retained_when_new_nodes_are_appended() {
    let nodes = [
        candidate_node(Some("node_current"), "Current", "brief.md", "current.md"),
        candidate_node(
            Some("node_future_one"),
            "Future one",
            "current.md",
            "one.md",
        ),
        candidate_node(Some("node_future_two"), "Future two", "one.md", "two.md"),
        candidate_node(None, "New final", "two.md", "final.md"),
    ]
    .join("\n");

    let plan =
        plan_workflow_definition_change(&accepted_definition(), &candidate_with_nodes(&nodes))
            .expect("plan append");

    let DefinitionChangePlan::Changed {
        retained_node_ids,
        retired_node_ids,
        introduced_nodes,
    } = plan
    else {
        panic!("expected changed plan");
    };
    assert_eq!(
        retained_node_ids,
        ["node_current", "node_future_one", "node_future_two"]
    );
    assert!(retired_node_ids.is_empty());
    assert_eq!(introduced_nodes.len(), 1);
}

#[test]
fn candidate_parser_rejects_invalid_bytes_syntax_and_fields() {
    for (candidate, expected) in [
        (vec![0xff], "valid UTF-8"),
        (b"not valid = [".to_vec(), "invalid candidate TOML"),
        (
            candidate_with_nodes(&format!(
                "unknown = true\n{}",
                candidate_node(Some("node_current"), "Current", "brief.md", "current.md")
            )),
            "unknown field",
        ),
        (
            candidate_with_nodes(&format!(
                "{}parent = \"node_other\"\n",
                candidate_node(Some("node_current"), "Current", "brief.md", "current.md")
            )),
            "unknown field",
        ),
    ] {
        let error = plan_workflow_definition_change(&accepted_definition(), &candidate)
            .expect_err("candidate must be rejected");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn candidate_rejects_unsafe_handoffs_and_invalid_agent_nodes() {
    let unsafe_handoff = candidate_with_nodes(&candidate_node(
        Some("node_current"),
        "Current",
        "../brief.md",
        "current.md",
    ));
    let unsafe_initial_handoff = String::from_utf8(candidate_with_nodes(&candidate_node(
        Some("node_current"),
        "Current",
        "brief.md",
        "current.md",
    )))
    .expect("candidate text")
    .replace("name = \"brief.md\"", "name = \"../brief.md\"")
    .into_bytes();
    let unsupported_node = candidate_with_nodes(
        &candidate_node(Some("node_current"), "Current", "brief.md", "current.md")
            .replace("type = \"agent\"", "type = \"control\""),
    );

    for (candidate, expected) in [
        (unsafe_handoff, "invalid Handoff file name"),
        (unsafe_initial_handoff, "invalid Handoff file name"),
        (unsupported_node, "unsupported Workflow Node type"),
    ] {
        let error = plan_workflow_definition_change(&accepted_definition(), &candidate)
            .expect_err("candidate must be rejected");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn candidate_cannot_change_accepted_metadata_or_protected_nodes() {
    let unchanged_nodes = [
        candidate_node(Some("node_current"), "Current", "brief.md", "current.md"),
        candidate_node(
            Some("node_future_one"),
            "Future one",
            "current.md",
            "one.md",
        ),
        candidate_node(Some("node_future_two"), "Future two", "one.md", "two.md"),
    ]
    .join("\n");
    let base = String::from_utf8(candidate_with_nodes(&unchanged_nodes)).expect("candidate text");
    let cases = [
        (base.replace("wf_plan", "wf_other"), "Workflow identity"),
        (
            base.replace("revision = 2", "revision = 1"),
            "candidate revision",
        ),
        (
            base.replace("/workspace/project", "/workspace/other"),
            "launch directory",
        ),
        (
            base.replace(
                "source = \"handoff/brief.md\"",
                "source = \"handoff/other.md\"",
            ),
            "initial Workflow Handoffs",
        ),
        (
            base.replace("id = \"node_current\"\n", ""),
            "protected Workflow Node",
        ),
        (
            base.replace("title = \"Current\"", "title = \"Changed current\""),
            "retained Workflow Node node_current definition",
        ),
    ];

    for (candidate, expected) in cases {
        let error = plan_workflow_definition_change(&accepted_definition(), candidate.as_bytes())
            .expect_err("immutable candidate change must fail");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn retained_nodes_require_known_active_identity_definition_and_parent() {
    let current = candidate_node(Some("node_current"), "Current", "brief.md", "current.md");
    let cases = [
        (
            [
                current.clone(),
                candidate_node(Some("node_retired"), "Future one", "current.md", "one.md"),
            ]
            .join("\n"),
            "retired Workflow Node identity",
        ),
        (
            [
                current.clone(),
                candidate_node(Some("node_invented"), "Future one", "current.md", "one.md"),
            ]
            .join("\n"),
            "caller-provided Workflow Node identity",
        ),
        (
            [
                current.clone(),
                candidate_node(Some("node_future_one"), "Changed", "current.md", "one.md"),
            ]
            .join("\n"),
            "definition cannot be changed",
        ),
        (
            [
                current,
                candidate_node(None, "Replacement one", "current.md", "one.md"),
                candidate_node(Some("node_future_two"), "Future two", "one.md", "two.md"),
            ]
            .join("\n"),
            "changed immutable parent",
        ),
    ];

    for (nodes, expected) in cases {
        let error =
            plan_workflow_definition_change(&accepted_definition(), &candidate_with_nodes(&nodes))
                .expect_err("invalid identity retention must fail");
        assert!(error.to_string().contains(expected), "{error}");
    }
}
