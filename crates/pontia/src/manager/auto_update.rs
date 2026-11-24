use std::path::Path;

use pontia_runtime::local_service::{
    DefinitionStore, absolute_utf8_path, remove_file_if_present, systemd_quote, xml_escape,
};

use super::{CommandRunner, LaunchdManager, SystemdManager, command_failure, property};
use crate::runtime_io::FileDefinitionStore;

const TIMER: &str = "pontia-update.timer";
const SERVICE: &str = "pontia-update.service";
const LABEL: &str = "dev.pontia.update";
const TIMER_CONTENTS: &str = "[Unit]
Description=Check for Pontia updates every six hours

[Timer]
OnCalendar=*-*-* 00/6:00:00
RandomizedDelaySec=10m
Persistent=true

[Install]
WantedBy=timers.target
";

impl<R: CommandRunner> SystemdManager<'_, R> {
    pub fn enable_auto_update(&self, directory: &Path, cli: &Path) -> Result<(), String> {
        let cli = systemd_quote(absolute_utf8_path(cli, "pontia executable")?);
        let service = format!(
            "[Unit]\nDescription=Update Pontia\n\n[Service]\nType=oneshot\nExecStart=\"{cli}\" update\n"
        );
        FileDefinitionStore.install(&directory.join(SERVICE), &service)?;
        FileDefinitionStore.install(&directory.join(TIMER), TIMER_CONTENTS)?;
        self.require_systemctl(&["daemon-reload"])?;
        self.require_systemctl(&["enable", "--now", TIMER])
    }

    pub fn disable_auto_update(&self, directory: &Path) -> Result<(), String> {
        let args = ["disable", "--now", TIMER];
        let output = self.systemctl(&args)?;
        if output.code != 0 && !(output.stderr.contains(TIMER) && super::is_missing(&output)) {
            return Err(command_failure("systemctl", &args, &output));
        }
        remove_file_if_present(&directory.join(TIMER))?;
        remove_file_if_present(&directory.join(SERVICE))?;
        self.require_systemctl(&["daemon-reload"])
    }

    pub fn auto_update_status(&self) -> Result<String, String> {
        let args = [
            "show",
            TIMER,
            "--property=LoadState,UnitFileState,ActiveState,LastTriggerUSec,NextElapseUSecRealtime",
        ];
        let output = self.systemctl(&args)?;
        if property(&output.stdout, "LoadState") == Some("not-found") {
            return Ok("Automatic updates disabled (timer is not installed).".into());
        }
        if output.code != 0 {
            return Err(command_failure("systemctl", &args, &output));
        }
        let value = |name| property(&output.stdout, name).filter(|s| !s.is_empty());
        Ok(format!(
            "Automatic updates: {} (timer {}).\nLast run: {}\nNext run: {}",
            value("UnitFileState").unwrap_or("unknown"),
            value("ActiveState").unwrap_or("unknown"),
            value("LastTriggerUSec").unwrap_or("unavailable"),
            value("NextElapseUSecRealtime").unwrap_or("not scheduled"),
        ))
    }
}

impl<R: CommandRunner> LaunchdManager<'_, R> {
    fn update_job_loaded(&self, target: &str) -> Result<bool, String> {
        let args = ["print", target];
        let output = self.launchctl(args.map(String::from).into())?;
        if output.code == 0 {
            Ok(true)
        } else if output
            .stderr
            .to_ascii_lowercase()
            .contains("could not find service")
        {
            Ok(false)
        } else {
            Err(command_failure("launchctl", &args, &output))
        }
    }

    pub fn enable_auto_update(&self, directory: &Path, cli: &Path) -> Result<(), String> {
        let cli = xml_escape(absolute_utf8_path(cli, "pontia executable")?)?;
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array><string>{cli}</string><string>update</string></array>
    <key>StartInterval</key><integer>21600</integer>
</dict>
</plist>
"#
        );
        let path = directory.join(format!("{LABEL}.plist"));
        let domain = self.domain();
        let target = format!("{domain}/{LABEL}");
        let loaded = self.update_job_loaded(&target)?;
        self.require_launchctl(vec!["enable".into(), target.clone()])?;
        if loaded {
            if FileDefinitionStore.read(&path)?.as_deref() == Some(&plist) {
                return Ok(());
            }
            self.require_launchctl(vec!["bootout".into(), target])?;
        }
        FileDefinitionStore.install(&path, &plist)?;
        self.require_launchctl(vec![
            "bootstrap".into(),
            domain,
            absolute_utf8_path(&path, "launchd plist")?.into(),
        ])
    }

    pub fn disable_auto_update(&self, directory: &Path) -> Result<(), String> {
        let target = format!("{}/{LABEL}", self.domain());
        if self.update_job_loaded(&target)? {
            self.require_launchctl(vec!["bootout".into(), target])?;
        }
        remove_file_if_present(&directory.join(format!("{LABEL}.plist")))
    }

    pub fn auto_update_status(&self) -> Result<String, String> {
        let domain = self.domain();
        if !self.update_job_loaded(&format!("{domain}/{LABEL}"))? {
            return Ok("Automatic updates disabled (launchd job is not loaded).".into());
        }
        let args = ["print-disabled", &domain];
        let output = self.launchctl(args.map(String::from).into())?;
        if output.code != 0 {
            return Err(command_failure("launchctl", &args, &output));
        }
        let disabled = output
            .stdout
            .lines()
            .filter_map(|line| line.split_once("=>"))
            .any(|(label, state)| {
                label.trim().trim_matches('"') == LABEL
                    && matches!(state.trim(), "disabled" | "true")
            });
        let state = if disabled {
            "disabled for future loads"
        } else {
            "enabled"
        };
        Ok(format!(
            "Automatic updates {state} (launchd job loaded, every six hours).\nLast and next run times are unavailable for launchd interval jobs."
        ))
    }
}
