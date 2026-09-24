use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use pontia_application::clients::{ClientLaunchRequest, ClientLauncher};
use pontia_core::{Error, Result};

pub(crate) struct PiLauncher {
    pub tui_command: Option<String>,
}
impl ClientLauncher for PiLauncher {
    fn validate_resume(&self, key: &str, file: Option<&Path>) -> Result<()> {
        let path = file.ok_or_else(|| {
            Error::CapabilityUnavailable(format!(
                "cannot resume Pi session {key}: binding has no native session file"
            ))
        })?;
        verify_session_file(path, key)
    }

    fn launch(
        &self,
        request: ClientLaunchRequest<'_>,
    ) -> Result<pontia_runtime::RuntimeStartResult> {
        let mut runtime = request.runtime;
        let command = runtime.start_command.take().unwrap_or_else(|| {
            let command = std::env::var("PONTIA_PI_TUI_COMMAND")
                .ok()
                .or_else(|| self.tui_command.clone())
                .unwrap_or_else(|| "pi".into());
            format!("{command} --approve")
        });
        let session_args = match request.native_session_key {
            Some(key) => {
                self.validate_resume(key, request.native_session_file)?;
                let path = request
                    .native_session_file
                    .expect("validated native session file");
                format!("--session {}", quote(&path.to_string_lossy()))
            }
            None => format!("--session-id {}", quote(&runtime.session_id)),
        };
        runtime.start_command = Some(format!("{command} {session_args}"));
        let mut result = pontia_runtime::GenericRuntimeManager.start_tmux(
            request.root,
            runtime,
            request.restart_count,
            request.reuse_pane,
            &crate::SPEC.launch_options(),
        )?;
        // Persist the reusable command, not the selector for this launch's native session.
        result.metadata["start_command"] = command.into();
        Ok(result)
    }
}

fn verify_session_file(path: &Path, key: &str) -> Result<()> {
    let read_header = || -> Result<serde_json::Value> {
        let mut line = String::new();
        BufReader::new(File::open(path)?).read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    };
    let header = read_header().map_err(|error| {
        Error::CapabilityUnavailable(format!(
            "cannot resume Pi session from {}: {error}",
            path.display()
        ))
    })?;
    if header["type"] != "session" || header["id"].as_str() != Some(key) {
        return Err(Error::StateConflict(format!(
            "Pi session file {} does not match native session {key}",
            path.display()
        )));
    }
    Ok(())
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
