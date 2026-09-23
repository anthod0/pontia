use pontia_application::clients::{ClientLaunchRequest, ClientLauncher};
use pontia_core::Result;

pub(crate) struct PiLauncher {
    pub tui_command: Option<String>,
}
impl ClientLauncher for PiLauncher {
    fn launch(
        &self,
        request: ClientLaunchRequest<'_>,
    ) -> Result<pontia_runtime::RuntimeStartResult> {
        let mut runtime = request.runtime;
        runtime.start_command = Some(
            match (runtime.start_command.as_ref(), request.native_session_key) {
                (Some(command), Some(key)) => format!("{command} --session-id {}", quote(key)),
                (Some(command), None) => command.clone(),
                (None, key) => {
                    let command = std::env::var("PONTIA_PI_TUI_COMMAND")
                        .ok()
                        .or_else(|| self.tui_command.clone())
                        .unwrap_or_else(|| "pi".into());
                    format!(
                        "{command} --approve --session-id {}",
                        quote(key.unwrap_or(&runtime.session_id))
                    )
                }
            },
        );
        pontia_runtime::GenericRuntimeManager.start_tmux(
            request.root,
            runtime,
            request.restart_count,
            request.reuse_pane,
            &crate::SPEC.launch_options(),
        )
    }
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
