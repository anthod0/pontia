use std::{collections::HashMap, path::Path};

use pontia::{
    lifecycle::{DefinitionStore, EnabledState, HealthProbe, RunState, ServiceManager},
    runtime_io::{FileDefinitionStore, HttpHealthProbe},
};

use crate::{login, remote};

struct Check {
    passed: Option<bool>,
    summary: String,
    explanation: String,
}

impl Check {
    fn print(&self, verbose: bool) {
        let marker = match self.passed {
            Some(true) => "✓",
            Some(false) => "✗",
            None => "-",
        };
        let text = if verbose {
            &self.explanation
        } else {
            &self.summary
        };
        // Cloud names and diagnostic errors must stay on one terminal line.
        let text: String = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        println!(" {marker} {text}");
    }
}

pub(crate) async fn run(
    manager: &impl ServiceManager,
    user_home: &Path,
    vars: &HashMap<String, String>,
    verbose: bool,
) -> Result<bool, String> {
    let definition_path = manager.definition_path(user_home);
    let definition = FileDefinitionStore.read(&definition_path);
    let persisted_home = definition
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|definition| {
            definition
                .as_deref()
                .map(|text| manager.persisted_home(text))
                .transpose()
        });
    let home = match &persisted_home {
        Ok(Some(home)) => home.clone(),
        _ => login::pontia_home(vars)?,
    };
    let mut vars = vars.clone();
    vars.insert("PONTIA_HOME".into(), home.display().to_string());
    if verbose {
        println!(
            " - Pontia stores configuration and data in {} (PONTIA_HOME).",
            home.display()
        );
    } else {
        println!("PONTIA_HOME: {}", home.display());
    }

    let service = manager.status();
    let config = pontia::local_config::load(&vars);
    let http = config.as_ref().map_err(Clone::clone).and_then(|config| {
        HttpHealthProbe.is_healthy(pontia::lifecycle::local_health_addr(config.bind_addr))
    });
    let healthy = service
        .as_ref()
        .is_ok_and(|s| s.run_state == RunState::Running)
        && matches!(http, Ok(true))
        && persisted_home.is_ok();
    Check {
        passed: Some(healthy),
        summary: format!("state: {}", if healthy { "healthy" } else { "unhealthy" }),
        explanation: if healthy {
            "The pontiad process is running and its local HTTP health check passes.".into()
        } else {
            "Pontia is not healthy; review the service and local HTTP checks below.".into()
        },
    }
    .print(verbose);

    if verbose {
        println!(
            " - Pontia CLI version {} is running on {}.",
            pontia_version::version(),
            std::env::consts::OS
        );
        let explanation = match &persisted_home {
            Ok(Some(_)) => format!(
                "The system service configuration that starts pontiad is installed at {}.",
                definition_path.display()
            ),
            Ok(None) => format!(
                "The system service configuration is missing at {}; run `pontia up` to install it.",
                definition_path.display()
            ),
            Err(error) => format!(
                "The system service configuration at {} could not be read: {error}.",
                definition_path.display()
            ),
        };
        Check {
            passed: Some(matches!(persisted_home, Ok(Some(_)))),
            summary: String::new(),
            explanation,
        }
        .print(true);
        match &service {
            Ok(service) => {
                let running = match service.run_state {
                    RunState::Running => "running",
                    RunState::Stopped => "stopped; run `pontia up` to start it",
                    RunState::Starting => "starting",
                    RunState::Failed => "failed; inspect the runtime log and run `pontia up` after fixing the cause",
                };
                Check { passed: Some(service.run_state == RunState::Running), summary: String::new(), explanation: format!("The system service manager reports that the pontiad process is {running}.") }.print(true);
                let enabled = match service.enabled {
                    EnabledState::Enabled => "enabled",
                    EnabledState::Disabled => "disabled; run `pontia up` to enable it",
                    EnabledState::Unknown => "unknown to the service manager",
                };
                Check { passed: if service.enabled == EnabledState::Enabled { Some(true) } else { None }, summary: String::new(), explanation: format!("Automatic startup of the Pontia user service is {enabled}.") }.print(true);
            }
            Err(error) => Check { passed: Some(false), summary: String::new(), explanation: format!("The system service manager could not report whether pontiad is running: {error}.") }.print(true),
        }
        let config_path = home.join("config.toml");
        let explanation = match &config {
            Ok(_) if config_path.exists() => format!(
                "Pontia settings were loaded successfully from {}.",
                config_path.display()
            ),
            Ok(_) => format!(
                "No configuration file exists at {}; built-in defaults are in use.",
                config_path.display()
            ),
            Err(error) => format!(
                "Pontia settings could not be loaded from {}: {error}.",
                config_path.display()
            ),
        };
        Check {
            passed: Some(config.is_ok()),
            summary: String::new(),
            explanation,
        }
        .print(true);
        if let Ok(config) = &config {
            let addr = pontia::lifecycle::local_health_addr(config.bind_addr);
            let result = match &http {
                Ok(true) => "returned the expected healthy response".to_string(),
                Ok(false) => "did not return the expected healthy response".to_string(),
                Err(error) => format!("could not be completed: {error}"),
            };
            Check { passed: Some(matches!(http, Ok(true))), summary: String::new(), explanation: format!("The local HTTP check at http://{addr}/healthz {result}; it does not test remote access.") }.print(true);
            Check {
                passed: config.external_api_token.as_ref().map(|_| true),
                summary: String::new(),
                explanation: if config.external_api_token.is_some() {
                    "A local API token is configured for Dashboard and CLI access.".into()
                } else {
                    "No local API token is configured; run `pontia up` to create one.".into()
                },
            }
            .print(true);
        } else {
            println!(
                " - The local HTTP check was skipped because the service address could not be read from the configuration."
            );
        }
        println!(
            " - Runtime messages are written to {}.",
            home.join("state/runtime.log").display()
        );
        println!(
            " - The cloud login credential is stored at {}.",
            home.join("auth.json").display()
        );
        match login::auth_origin(&vars) {
            Ok(origin) => println!(" - Login and device registration are checked with {origin}."),
            Err(_) => println!(" ✗ The cloud address is invalid; check PONTIA_AUTH_ORIGIN."),
        }
    }

    let account = match login::account(&vars).await {
        Ok(Some(account)) => Check {
            passed: Some(true),
            summary: format!("account {}", account.name()),
            explanation: format!(
                "The cloud confirms that you are signed in as {} (user ID {}).",
                account.name(),
                account.id
            ),
        },
        Ok(None) => Check {
            passed: None,
            summary: "account: not logged in".into(),
            explanation: "No cloud login credential is saved; run `pontia login` to sign in."
                .into(),
        },
        Err(error) => Check {
            passed: Some(false),
            summary: format!("account: {error}"),
            explanation: format!("Cloud login could not be verified: {error}."),
        },
    };
    account.print(verbose);

    let registration = if config.is_ok() {
        remote::registration_status(&vars).await
    } else {
        Err("configuration could not be loaded; fix config.toml and retry".into())
    };
    match registration {
        Ok(remote::RegistrationStatus::Disabled) => {
            Check { passed: None, summary: "remote: disabled".into(), explanation: "Remote access is not configured; run `pontia remote enable` to register this device.".into() }.print(verbose);
            Check {
                passed: None,
                summary: "edge: not selected".into(),
                explanation: "No edge is selected because remote access is not configured.".into(),
            }
            .print(verbose);
        }
        Ok(remote::RegistrationStatus::Missing { device_id }) => {
            Check { passed: Some(false), summary: "remote: registration missing".into(), explanation: format!("Device {device_id} is not registered in the cloud; run `pontia remote enable` to register it.") }.print(verbose);
            Check {
                passed: None,
                summary: "edge: unavailable".into(),
                explanation:
                    "The assigned edge cannot be determined without a cloud device registration."
                        .into(),
            }
            .print(verbose);
        }
        Ok(remote::RegistrationStatus::Registered { device, identity }) => {
            Check { passed: Some(true), summary: format!("remote: registered as {}", device.handle), explanation: format!("The cloud has registered this device as {} (name {}, ID {}); registration alone does not confirm a live remote connection.", device.handle, device.name, device.id) }.print(verbose);
            Check {
                passed: Some(true),
                summary: format!("edge: {}", device.edge_name),
                explanation: format!(
                    "The device is assigned to edge {} (ID {}); its connectivity was not tested.",
                    device.edge_name, device.edge_id
                ),
            }
            .print(verbose);
            if verbose {
                println!(
                    " - Open https://app.pontia.dev/{} to access this device's Remote Dashboard.",
                    device.handle
                );
                let explanation = match &identity {
                    Ok(true) => format!("The local encryption identity matches cloud key version {}.", device.e2e_key_version),
                    Ok(false) => "The local encryption identity is missing or does not match the cloud; run `pontia remote enable` to repair it.".into(),
                    Err(error) => format!("The local encryption identity could not be checked: {error}."),
                };
                Check {
                    passed: Some(matches!(identity, Ok(true))),
                    summary: String::new(),
                    explanation,
                }
                .print(true);
            }
        }
        Err(error) => {
            Check {
                passed: Some(false),
                summary: format!("remote: {error}"),
                explanation: format!("Cloud device registration could not be checked: {error}."),
            }
            .print(verbose);
            Check { passed: None, summary: "edge: unknown".into(), explanation: "The assigned edge is unknown because the device registration could not be checked.".into() }.print(verbose);
        }
    }
    Ok(healthy)
}
