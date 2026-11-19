use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use pontia_agent_control::client_contract::ApplicationLaunchSettings;
use pontia_agent_control::clients::{ClientLaunchRequest, ClientLauncher};
use pontia_core::{Error, Result};

pub(crate) struct PiLauncher {
    pub tui_command: Option<String>,
}
impl ClientLauncher for PiLauncher {
    fn supports_application_launch(&self) -> bool {
        true
    }

    fn application_command(
        &self,
        configuration: &serde_json::Value,
        settings: &ApplicationLaunchSettings,
    ) -> Result<String> {
        let command = crate::config::command(configuration)?;
        let project_settings = settings.directory.join(".pi");
        std::fs::create_dir_all(&project_settings)?;
        let settings_file = project_settings.join("settings.json");
        let mut local: serde_json::Map<String, serde_json::Value> =
            match std::fs::read(&settings_file) {
                Ok(content) => serde_json::from_slice(&content)?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Default::default(),
                Err(error) => return Err(error.into()),
            };
        if local
            .get("steeringMode")
            .and_then(serde_json::Value::as_str)
            != Some("all")
        {
            local.insert("steeringMode".into(), "all".into());
            let file = tempfile::NamedTempFile::new_in(&project_settings)?;
            serde_json::to_writer_pretty(file.as_file(), &local)?;
            file.persist(&settings_file).map_err(|error| error.error)?;
        }
        Ok(format!(
            "export PONTIA_PI_MANAGED_SESSION=1; {command} --approve --model {} --append-system-prompt {}",
            quote(&settings.model),
            quote(&settings.instructions_file.to_string_lossy()),
        ))
    }

    fn launch_configured(
        &self,
        request: ClientLaunchRequest<'_>,
        configuration: &serde_json::Value,
    ) -> Result<pontia_runtime::RuntimeStartResult> {
        let mut request = request;
        request.runtime.start_command = Some(format!(
            "{} --approve",
            crate::config::command(configuration)?
        ));
        self.launch(request)
    }

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
        let command = match &runtime.model {
            Some(model) => format!("{command} --model {}", quote(model)),
            None => command,
        };
        // Pi runs tools without approval or sandboxing. --approve trusts project resources.
        let mut policy_args = match &runtime.execution_policy.tools {
            None => String::new(),
            Some(tools) if tools.is_empty() => " --no-tools".into(),
            Some(tools) => format!(" --tools {}", quote(&tools.join(","))),
        };
        if !runtime.execution_policy.disabled_tools.is_empty() {
            policy_args.push_str(&format!(
                " --exclude-tools {}",
                quote(&runtime.execution_policy.disabled_tools.join(","))
            ));
        }
        if runtime.execution_policy.disable_mcp {
            policy_args.push_str(" --no-mcp");
        }
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
        runtime.start_command = Some(format!("{command}{policy_args} {session_args}"));
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
