use pontia_agent_control::client_contract::LaunchFile;

pub(crate) const FILES: &[LaunchFile] = &[
    LaunchFile {
        path: ".kiro/settings/cli.json",
        contents: r#"{"chat.disableInheritingDefaultResources":true}"#,
    },
    LaunchFile {
        path: ".kiro/agents/pontia-restricted.json",
        contents: include_str!("restricted.json"),
    },
];
