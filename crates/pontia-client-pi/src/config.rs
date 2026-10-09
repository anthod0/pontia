use pontia_config::{ClientConfigDefaults, RuntimeCommandOverride};

pub const DEFAULTS: ClientConfigDefaults = ClientConfigDefaults {
    default_client_type: "pi",
    runtime_commands: &[RuntimeCommandOverride {
        client_type: "pi",
        environment_variable: "PONTIA_PI_TUI_COMMAND",
    }],
};
