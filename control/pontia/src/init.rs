use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufRead, ErrorKind, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use pontia_config::{AppConfig, WorkspaceRootConfig};
use toml_edit::{Array, DocumentMut, InlineTable, Item, Value};

static TEMP_SEQUENCE: AtomicUsize = AtomicUsize::new(0);

pub trait InitPlatform {
    fn preflight(&self, install_pi: bool) -> Result<(), String>;
    fn fill_random(&self, bytes: &mut [u8]) -> Result<(), String>;
    fn install_pi(&self) -> Result<(), String>;
    fn start_service(&self, config: &AppConfig, config_changed: bool) -> Result<(), String>;
    fn dashboard_available(&self, addr: SocketAddr) -> Result<bool, String>;
    fn open_browser(&self, url: &str) -> Result<(), String>;
}

pub fn run<R: BufRead, W: Write, P: InitPlatform>(
    input: &mut R,
    output: &mut W,
    vars: &HashMap<String, String>,
    platform: &P,
) -> Result<(), String> {
    let persistent_vars = persistent_vars(vars);
    let existing = load_persistent_config(&persistent_vars)?;
    if existing.bind_addr.port() == 0 {
        return Err(
            "bind_addr port 0 is not supported by pontia init; configure a concrete service port"
                .to_string(),
        );
    }
    let user_home = validated_user_home(vars)?;
    let initial_roots = if existing.workspace_browser.roots.is_empty() {
        vec![WorkspaceRootConfig {
            root_id: "home".to_string(),
            label: "Home".to_string(),
            path: user_home.display().to_string(),
        }]
    } else {
        existing.workspace_browser.roots.clone()
    };

    writeln!(output, "Pontia initialization\n").map_err(io_error)?;
    writeln!(output, "Select Agent Clients:\n  [x] pi").map_err(io_error)?;
    write!(
        output,
        "Press Enter to install all, or type 'none' to skip: "
    )
    .map_err(io_error)?;
    let install_pi = match read_answer(input, output)?.trim() {
        "" | "pi" => true,
        "none" => false,
        other => return Err(format!("unsupported Agent Client selection: {other}")),
    };

    writeln!(output, "\nWorkspace Browser roots:").map_err(io_error)?;
    for root in &initial_roots {
        writeln!(output, "  [x] {}", root.path).map_err(io_error)?;
    }
    write!(
        output,
        "Press Enter to keep these roots, type comma-separated absolute paths to replace them, or 'none': "
    )
    .map_err(io_error)?;
    let root_answer = read_answer(input, output)?;
    let roots = selected_roots(root_answer.trim(), &initial_roots)?;

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
        if install_pi { "install" } else { "skip" }
    )
    .map_err(io_error)?;
    writeln!(output, "  Workspace Browser roots: {}", roots.len()).map_err(io_error)?;
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
            return Ok(());
        }
        answer => return Err(format!("expected yes or no, got {answer:?}")),
    }

    platform.preflight(install_pi)?;
    if install_pi {
        platform.install_pi()?;
        writeln!(output, "✓ Installed pi integration").map_err(io_error)?;
    }

    let config_path = existing.pontia_home.join("config.toml");
    let config_changed = write_config(
        &config_path,
        &token,
        &roots,
        existing.external_api_token.as_deref(),
        &existing.workspace_browser.roots,
    )?;
    writeln!(output, "✓ Wrote {}", config_path.display()).map_err(io_error)?;

    let config = load_persistent_config(&persistent_vars)?;
    platform.start_service(&config, config_changed)?;
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
    let url = url.to_string();
    writeln!(output, "Dashboard: {url}").map_err(io_error)?;
    match platform.open_browser(&url) {
        Ok(()) => writeln!(output, "✓ Opened Dashboard in your browser").map_err(io_error)?,
        Err(error) => writeln!(output, "Could not open Dashboard: {error}").map_err(io_error)?,
    }
    writeln!(
        output,
        "Press Enter to open Dashboard again. Press Ctrl-C to exit initialization."
    )
    .map_err(io_error)?;
    writeln!(
        output,
        "Pontia will keep running. Use `pontia down` to stop it."
    )
    .map_err(io_error)?;

    loop {
        let mut line = String::new();
        match input.read_line(&mut line).map_err(io_error)? {
            0 => return Ok(()),
            _ if line.trim().is_empty() => match platform.open_browser(&url) {
                Ok(()) => {
                    writeln!(output, "✓ Opened Dashboard in your browser").map_err(io_error)?
                }
                Err(error) => {
                    writeln!(output, "Could not open Dashboard: {error}").map_err(io_error)?
                }
            },
            _ => writeln!(
                output,
                "Press Enter to open Dashboard again. Press Ctrl-C to exit initialization."
            )
            .map_err(io_error)?,
        }
        output.flush().map_err(io_error)?;
    }
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

fn selected_roots(
    answer: &str,
    initial: &[WorkspaceRootConfig],
) -> Result<Vec<WorkspaceRootConfig>, String> {
    if answer.is_empty() {
        for root in initial {
            validate_root_path(Path::new(&root.path))?;
        }
        return Ok(initial.to_vec());
    }
    if answer == "none" {
        return Ok(Vec::new());
    }

    let mut used_ids = HashSet::new();
    answer
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .map(|path| {
            validate_root_path(&path)?;
            let existing = initial.iter().find(|root| Path::new(&root.path) == path);
            let label = existing
                .map(|root| root.label.clone())
                .unwrap_or_else(|| root_label(&path));
            let base_id = existing
                .map(|root| root.root_id.clone())
                .unwrap_or_else(|| root_id(&label));
            let root_id = unique_id(base_id, &mut used_ids);
            Ok(WorkspaceRootConfig {
                root_id,
                label,
                path: path.display().to_string(),
            })
        })
        .collect()
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

fn root_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Root")
        .to_string()
}

fn root_id(label: &str) -> String {
    let value = label
        .chars()
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let value = value.trim_matches('-');
    if value.is_empty() {
        "root".to_string()
    } else {
        value.to_string()
    }
}

fn unique_id(base: String, used: &mut HashSet<String>) -> String {
    if used.insert(base.clone()) {
        return base;
    }
    for suffix in 2.. {
        let candidate = format!("{base}-{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!()
}

fn generate_token<P: InitPlatform>(platform: &P) -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    platform.fill_random(&mut bytes)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn write_config(
    path: &Path,
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
    atomic_write_private(path, updated.as_bytes())?;
    Ok(changed)
}

fn atomic_write_private(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("configuration path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;

    for _ in 0..32 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!(
            ".pontia-config-{}-{sequence}.tmp",
            std::process::id()
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp_path) {
            Ok(mut file) => {
                let result = (|| -> std::io::Result<()> {
                    file.write_all(contents)?;
                    file.sync_all()?;
                    drop(file);
                    fs::rename(&temp_path, path)?;
                    File::open(parent)?.sync_all()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    let _ = fs::remove_file(&temp_path);
                    return Err(format!(
                        "failed to atomically write {}: {error}",
                        path.display()
                    ));
                }
                return Ok(());
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create a staging file for {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Err(format!(
        "failed to create a staging file for {}: too many name collisions",
        path.display()
    ))
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
