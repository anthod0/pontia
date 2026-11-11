use pontia_core::{
    Result,
    domain::{DomainEvent, SessionExecutionLifetime},
};

pub trait ClientEventInterpreter: Send + Sync {
    fn accompanying_runtime_event(&self, event: &DomainEvent) -> Result<Option<DomainEvent>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessObservationBehavior {
    pub role: &'static str,
    pub observe_starting: bool,
    pub exit_event: crate::PontiaEventType,
    pub exit_reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionLifecycleBehavior {
    pub execution_lifetime: SessionExecutionLifetime,
    pub coupled_runtime: bool,
    pub confirmed_exit_recovery: bool,
    pub restart_requires_exit: bool,
    pub independent_interface: bool,
    pub process_observation: Option<ProcessObservationBehavior>,
}

impl SessionLifecycleBehavior {
    pub const DEFAULT: Self = Self {
        execution_lifetime: SessionExecutionLifetime::Execution,
        coupled_runtime: false,
        confirmed_exit_recovery: false,
        restart_requires_exit: false,
        independent_interface: false,
        process_observation: None,
    };
}
