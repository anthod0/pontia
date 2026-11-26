use super::*;
use pontia_agent_control::client_contract::install::{
    AuthGuidance, AuthStatus, PrerequisiteStatus,
};

struct Installer {
    ready: bool,
}

impl ClientInstaller for Installer {
    fn client_type(&self) -> &'static str {
        "sample"
    }
    fn client_key(&self) -> &'static str {
        "default"
    }
    fn inspect(&self, _: &DetectionContext) -> InstallSpec {
        InstallSpec {
            provenance: InstallProvenance::NotInstalled,
            methods: vec![InstallMethod {
                id: "local".into(),
                label: "Local".into(),
                recommended: true,
                supported_on_platform: true,
                source_summary: "fixture".into(),
                target_summary: "fixture".into(),
                prerequisites: vec![PrerequisiteStatus {
                    id: "tool".into(),
                    label: "Tool".into(),
                    required: true,
                    satisfied: self.ready,
                    detected_detail: None,
                    missing_hint: Some("Install tool".into()),
                }],
            }],
            auth: AuthGuidance {
                status: AuthStatus::Unknown,
                login_command: None,
                headless_login_command: None,
                env_vars: vec![],
                docs_url: None,
            },
        }
    }
    fn install(&self, method: &str, op: &mut operations::InstallOperation) -> io::Result<()> {
        assert!(self.ready);
        assert_eq!(method, "local");
        assert_eq!(op.phase(), Phase::Preflight);
        op.advance(Phase::Installing)?;
        op.advance(Phase::Verifying)
    }
}

#[test]
fn preflight_controls_dispatch_and_installer_owns_verification() {
    let root = tempfile::tempdir().unwrap();
    let context = DetectionContext {
        vars: &Default::default(),
        pontia_home: root.path(),
    };
    for ready in [false, true] {
        let installer = Installer { ready };
        let state = operations::run(root.path(), "sample", "default", "local", |op| {
            install(&installer, &context, "local", op)?;
            op.advance(Phase::SettingUp)
        })
        .unwrap();
        if ready {
            assert_eq!(state.status, Status::Succeeded);
            assert_eq!(state.phase, Phase::Done);
        } else {
            assert_eq!(state.status, Status::Failed);
            assert_eq!(state.phase, Phase::Preflight);
            assert_eq!(state.error.as_deref(), Some("Install tool"));
        }
    }
}
