use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, BufRead, ErrorKind, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use dialoguer::{MultiSelect, console::Term};
use pontia_config::{AppConfig, WorkspaceRootConfig};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Value};

use crate::{codex::CodexSetup, private_file};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentSelection {
    pub pi: bool,
    pub codex: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitOutcome {
    pub local_dashboard_url: String,
}

pub trait InitPlatform {
    fn inspect_codex(
        &self,
        vars: &HashMap<String, String>,
        user_home: &Path,
    ) -> Result<CodexSetup, String>;
    fn preflight(&self, install_pi: bool) -> Result<(), String>;
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), String>;
    fn install_pi(&self) -> Result<(), String>;
    fn initialize_codex(&self, setup: &CodexSetup) -> Result<(), String>;
    fn start_service(
        &self,
        config: &AppConfig,
        config_changed: bool,
        codex_home: Option<&Path>,
    ) -> Result<(), String>;
    fn dashboard_available(&self, addr: SocketAddr) -> Result<bool, String>;
}

pub fn run<R: BufRead, W: Write, P: InitPlatform>(
    input: &mut R,
    output: &mut W,
    vars: &HashMap<String, String>,
    platform: &P,
) -> Result<Option<InitOutcome>, String> {
    run_with_selector(input, output, vars, platform, line_agent_selection)
}

pub fn run_interactive<P: InitPlatform>(
    vars: &HashMap<String, String>,
    platform: &P,
) -> Result<Option<InitOutcome>, String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_with_selector(
        &mut stdin.lock(),
        &mut stdout.lock(),
        vars,
        platform,
        interactive_agent_selection,
    )
}

fn run_with_selector<R, W, P, S>(
    input: &mut R,
    output: &mut W,
    vars: &HashMap<String, String>,
    platform: &P,
    select_agents: S,
) -> Result<Option<InitOutcome>, String>
where
    R: BufRead,
    W: Write,
    P: InitPlatform,
    S: FnOnce(&mut R, &mut W) -> Result<AgentSelection, String>,
{
    let persistent_vars = persistent_vars(vars);
    let existing = load_persistent_config(&persistent_vars)?;
    if existing.bind_addr.port() == 0 {
        return Err(
            "bind_addr port 0 is not supported by pontia init; configure a concrete service port"
                .to_string(),
        );
    }
    let user_home = validated_user_home(vars)?;
    let config_path = existing.pontia_home.join("config.toml");
    let config_exists = config_path
        .try_exists()
        .map_err(|error| format!("failed to inspect {}: {error}", config_path.display()))?;
    let initial_roots = if config_exists {
        existing.workspace_browser.roots.clone()
    } else {
        vec![WorkspaceRootConfig {
            root_id: "home".to_string(),
            label: "Home".to_string(),
            path: user_home.display().to_string(),
        }]
    };

    writeln!(output, "Pontia initialization\n").map_err(io_error)?;
    let agents = select_agents(input, output)?;
    let codex = agents
        .codex
        .then(|| platform.inspect_codex(vars, &user_home))
        .transpose()?;

    let token = match existing.external_api_token.as_deref() {
        Some(token) => token.to_string(),
        None => generate_token(platform)?,
    };

    let mut ignored_overrides = vars
        .iter()
        .filter(|(key, value)| {
            key.starts_with("PONTIA_") && key.as_str() != "PONTIA_HOME" && !value.trim().is_empty()
        })
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    ignored_overrides.sort_unstable();
    if !ignored_overrides.is_empty() {
        writeln!(
            output,
            "Warning: command-scoped overrides are not persisted to the Pontia service and will not be used by initialization: {}",
            ignored_overrides.join(", ")
        )
        .map_err(io_error)?;
    }

    writeln!(output, "\nInitialization summary:").map_err(io_error)?;
    writeln!(
        output,
        "  pi integration: {}",
        if agents.pi { "install" } else { "skip" }
    )
    .map_err(io_error)?;
    if let Some(codex) = &codex {
        writeln!(output, "  Codex integration: register autostart").map_err(io_error)?;
        writeln!(output, "  Codex executable: {}", codex.executable.display()).map_err(io_error)?;
        writeln!(output, "  CODEX_HOME: {}", codex.home.display()).map_err(io_error)?;
        writeln!(output, "  Codex service: {}", codex.service_path.display()).map_err(io_error)?;
        writeln!(output, "  user linger: enable for {}", codex.username).map_err(io_error)?;
    } else {
        writeln!(output, "  Codex integration: skip").map_err(io_error)?;
    }
    writeln!(output, "  Workspace Browser roots: {}", initial_roots.len()).map_err(io_error)?;
    writeln!(
        output,
        "  External API token: {}",
        if existing.external_api_token.is_some() {
            "keep existing"
        } else {
            "generate"
        }
    )
    .map_err(io_error)?;
    write!(output, "Continue? [Y/n]: ").map_err(io_error)?;
    match read_answer(input, output)?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "" | "y" | "yes" => {}
        "n" | "no" => {
            writeln!(output, "Initialization cancelled.").map_err(io_error)?;
            return Ok(None);
        }
        answer => return Err(format!("expected yes or no, got {answer:?}")),
    }

    platform.preflight(agents.pi)?;
    if agents.pi {
        platform.install_pi()?;
        writeln!(output, "✓ Installed pi integration").map_err(io_error)?;
    }
    if let Some(codex) = &codex {
        platform.initialize_codex(codex)?;
        writeln!(
            output,
            "✓ Configured Codex autostart and control connection"
        )
        .map_err(io_error)?;
    }

    let config_changed = write_config(
        &config_path,
        existing.bind_addr,
        &token,
        &initial_roots,
        existing.external_api_token.as_deref(),
        &existing.workspace_browser.roots,
    )?;
    writeln!(output, "✓ Wrote {}", config_path.display()).map_err(io_error)?;

    let config = load_persistent_config(&persistent_vars)?;
    platform.start_service(
        &config,
        config_changed,
        codex.as_ref().map(|setup| setup.home.as_path()),
    )?;
    writeln!(output, "✓ Started Pontia service").map_err(io_error)?;

    let dashboard_addr = local_addr(config.bind_addr);
    if !platform.dashboard_available(dashboard_addr)? {
        return Err(format!(
            "Dashboard is not available at http://{dashboard_addr}/dashboard"
        ));
    }
    let mut url = url::Url::parse(&format!("http://{dashboard_addr}/dashboard"))
        .map_err(|error| format!("failed to build Dashboard URL: {error}"))?;
    url.query_pairs_mut().append_pair("token", &token);
    Ok(Some(InitOutcome {
        local_dashboard_url: url.to_string(),
    }))
}

fn line_agent_selection<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
) -> Result<AgentSelection, String> {
    writeln!(output, "Select Agent Clients:\n  [x] pi\n  [ ] codex").map_err(io_error)?;
    write!(
        output,
        "Press Enter to keep the defaults, or type a selection ('pi', 'codex', 'pi,codex', or 'none'): "
    )
    .map_err(io_error)?;
    let answer = read_answer(input, output)?;
    let answer = answer.trim();
    if answer.is_empty() || answer == "pi" {
        return Ok(AgentSelection {
            pi: true,
            codex: false,
        });
    }
    if answer == "none" {
        return Ok(AgentSelection {
            pi: false,
            codex: false,
        });
    }
    let selected = answer
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<HashSet<_>>();
    if selected.is_empty()
        || selected
            .iter()
            .any(|value| !matches!(*value, "pi" | "codex"))
    {
        return Err(format!("unsupported Agent Client selection: {answer}"));
    }
    Ok(AgentSelection {
        pi: selected.contains("pi"),
        codex: selected.contains("codex"),
    })
}

fn interactive_agent_selection<R: BufRead, W: Write>(
    _input: &mut R,
    output: &mut W,
) -> Result<AgentSelection, String> {
    output.flush().map_err(io_error)?;
    let selected = MultiSelect::new()
        .with_prompt("Select Agent Clients (Space to toggle, Enter to confirm)")
        .items(["pi", "codex"])
        .defaults(&[true, false])
        .interact_on(&Term::stdout())
        .map_err(|error| format!("Agent Client selection failed: {error}"))?;
    Ok(AgentSelection {
        pi: selected.contains(&0),
        codex: selected.contains(&1),
    })
}

fn load_persistent_config(vars: &HashMap<String, String>) -> Result<AppConfig, String> {
    AppConfig::from_vars(vars).map_err(|_| {
        "failed to load Pontia configuration; verify PONTIA_HOME, config.toml syntax, and configured values"
            .to_string()
    })
}

fn persistent_vars(vars: &HashMap<String, String>) -> HashMap<String, String> {
    ["HOME", "PONTIA_HOME"]
        .into_iter()
        .filter_map(|key| vars.get(key).map(|value| (key.to_string(), value.clone())))
        .collect()
}

fn validated_user_home(vars: &HashMap<String, String>) -> Result<PathBuf, String> {
    let path = PathBuf::from(vars.get("HOME").ok_or_else(|| {
        "HOME must be set to install the per-user service and select the default Workspace Browser root"
            .to_string()
    })?);
    validate_root_path(&path)?;
    Ok(path)
}

fn read_answer<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> Result<String, String> {
    output.flush().map_err(io_error)?;
    let mut answer = String::new();
    if input.read_line(&mut answer).map_err(io_error)? == 0 {
        return Err("initialization input ended before confirmation".to_string());
    }
    Ok(answer)
}

fn validate_root_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(format!(
            "Workspace Browser root must be an existing absolute directory: {}",
            path.display()
        ));
    }
    Ok(())
}

fn generate_token<P: InitPlatform>(platform: &P) -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    platform.fill_random(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn write_config(
    path: &Path,
    bind_addr: SocketAddr,
    token: &str,
    roots: &[WorkspaceRootConfig],
    existing_token: Option<&str>,
    existing_roots: &[WorkspaceRootConfig],
) -> Result<bool, String> {
    let original = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    let mut document = if original.trim().is_empty() {
        DocumentMut::new()
    } else {
        original
            .parse::<DocumentMut>()
            .map_err(|error| format!("failed to parse {}: {error}", path.display()))?
    };
    if document.get("bind_addr").is_none() {
        document["bind_addr"] = toml_edit::value(bind_addr.to_string());
    }
    if existing_token != Some(token) || document.get("external_api_token").is_none() {
        document["external_api_token"] = toml_edit::value(token);
    }
    let roots_present = document
        .get("workspace_browser")
        .and_then(Item::as_table)
        .and_then(|table| table.get("roots"))
        .is_some();
    if existing_roots != roots || !roots_present {
        let mut array = Array::new();
        for root in roots {
            let mut table = InlineTable::new();
            table.insert("root_id", Value::from(root.root_id.as_str()));
            table.insert("label", Value::from(root.label.as_str()));
            table.insert("path", Value::from(root.path.as_str()));
            array.push(Value::InlineTable(table));
        }
        document["workspace_browser"]["roots"] = Item::Value(Value::Array(array));
    }
    let updated = document.to_string();
    let changed = updated != original;
    private_file::atomic_write(path, updated.as_bytes())?;
    Ok(changed)
}

fn local_addr(addr: SocketAddr) -> SocketAddr {
    let ip = if addr.ip().is_unspecified() {
        match addr.ip() {
            IpAddr::V4(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V6(_) => IpAddr::V6(Ipv6Addr::LOCALHOST),
        }
    } else {
        addr.ip()
    };
    SocketAddr::new(ip, addr.port())
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}
