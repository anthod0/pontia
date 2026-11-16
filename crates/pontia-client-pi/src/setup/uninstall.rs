use pontia_agent_control::client_contract::PreparedClientUninstall;
use pontia_runtime::local_service::CommandRunner;
use std::{collections::HashMap, fs, path::Path};

struct Uninstall {
    sources: Vec<String>,
    environment: Vec<(String, String)>,
}

pub(super) fn prepare(
    vars: &HashMap<String, String>,
    home: &Path,
    runner: &dyn CommandRunner,
) -> Result<Option<Box<dyn PreparedClientUninstall>>, String> {
    let directory = vars
        .get("PI_CODING_AGENT_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| home.join(".pi/agent"));
    let settings = match fs::read_to_string(directory.join("settings.json")) {
        Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|error| format!("invalid Pi settings: {error}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not read Pi settings: {error}")),
    };
    if settings
        .get("packages")
        .is_none_or(|packages| packages.as_array().is_some_and(Vec::is_empty))
    {
        return Ok(None);
    }
    let environment = vec![
        (
            "PI_CODING_AGENT_DIR".into(),
            directory.display().to_string(),
        ),
        ("NO_COLOR".into(), "1".into()),
    ];
    let output = runner.run_with_env("pi", &["list".into()], &environment)?;
    if output.code != 0 {
        return Err(format!("pi list failed: {}", output.stderr.trim()));
    }
    let mut sources = Vec::new();
    let mut current = None;
    for line in output.stdout.lines() {
        if line == "Project packages:" {
            break;
        }
        if let Some(path) = line.strip_prefix("    ") {
            let Some(source) = current.take() else {
                continue;
            };
            let manifest = match fs::read_to_string(Path::new(path.trim()).join("package.json")) {
                Ok(text) => Some(
                    serde_json::from_str::<serde_json::Value>(&text)
                        .map_err(|error| format!("invalid Pi package manifest: {error}"))?,
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
            if manifest.is_some_and(|manifest| manifest["name"] == "@pontia/pi-client-plugin")
                && !sources.contains(&source)
            {
                sources.push(source);
            }
        } else if let Some(source) = line.strip_prefix("  ") {
            let source = source.trim_end_matches(" (filtered)").to_string();
            if source == "npm:@pontia/pi-client-plugin"
                || source.starts_with("npm:@pontia/pi-client-plugin@")
            {
                sources.push(source.clone());
            }
            current = Some(source);
        }
    }
    Ok((!sources.is_empty()).then(|| {
        Box::new(Uninstall {
            sources,
            environment,
        }) as Box<dyn PreparedClientUninstall>
    }))
}

impl PreparedClientUninstall for Uninstall {
    fn summary(&self) -> Vec<String> {
        self.sources
            .iter()
            .map(|source| format!("Remove Pi integration: {source}"))
            .collect()
    }

    fn uninstall(&self, runner: &dyn CommandRunner) -> Result<(), String> {
        for source in &self.sources {
            let output =
                runner.run_with_env("pi", &["remove".into(), source.clone()], &self.environment)?;
            if output.code != 0 {
                return Err(format!(
                    "pi remove {source} failed: {}",
                    output.stderr.trim()
                ));
            }
        }
        Ok(())
    }
}
