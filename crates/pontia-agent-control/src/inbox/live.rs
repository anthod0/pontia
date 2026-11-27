use super::*;
use crate::{
    InputPayload,
    inbox::{SessionActor, SessionPhase},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendMode {
    Immediate,
    Queue,
    Interrupt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SendOutcome {
    Delivered,
    Queued { message_id: String },
}

enum ImmediateSendError {
    Control(Error),
    Delivery(crate::DeliveryError),
}

impl ImmediateSendError {
    fn into_delivery(self) -> crate::DeliveryError {
        match self {
            Self::Control(Error::Domain(message)) => crate::DeliveryError::StateChanged(message),
            Self::Control(error) => error.into(),
            Self::Delivery(error) => error,
        }
    }
}

fn immediate_result<T>(
    result: crate::control::ControlResult<T>,
) -> std::result::Result<T, ImmediateSendError> {
    match result {
        crate::control::ControlResult::Accepted(value)
        | crate::control::ControlResult::Sent(value) => Ok(value),
        crate::control::ControlResult::Rejected(error) => Err(ImmediateSendError::Control(error)),
        crate::control::ControlResult::Unknown(message) => Err(ImmediateSendError::Delivery(
            crate::DeliveryError::Uncertain(message),
        )),
        crate::control::ControlResult::Unsupported(message) => Err(ImmediateSendError::Delivery(
            crate::DeliveryError::Unsupported(message),
        )),
    }
}

impl InboxCommandService {
    pub async fn send(
        &self,
        session_id: &str,
        input: InputPayload,
        mode: SendMode,
    ) -> std::result::Result<SendOutcome, crate::DeliveryError> {
        if input.text.trim().is_empty() {
            return Err(crate::DeliveryError::StateChanged(
                "input must not be blank".into(),
            ));
        }
        let actor = self.scheduler.session(session_id);
        let _guard = actor.lock().await;
        match mode {
            SendMode::Immediate => self
                .deliver_immediate_locked(session_id, input, &actor)
                .await
                .map_err(ImmediateSendError::into_delivery),
            SendMode::Queue => {
                self.queue_input_locked(session_id, input, "after_idle", true, &actor)
                    .await
            }
            SendMode::Interrupt => self.interrupt_input_locked(session_id, input, &actor).await,
        }
    }

    async fn deliver_immediate_locked(
        &self,
        session_id: &str,
        input: InputPayload,
        actor: &SessionActor,
    ) -> std::result::Result<SendOutcome, ImmediateSendError> {
        let session = self
            .queries
            .get_session_control(session_id)
            .await
            .map_err(ImmediateSendError::Control)?
            .ok_or_else(|| {
                ImmediateSendError::Delivery(crate::DeliveryError::StateChanged(format!(
                    "session {session_id} not found"
                )))
            })?;
        let active = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await
            .map_err(ImmediateSendError::Control)?;
        if let Some(turn) = active {
            actor.set_phase(SessionPhase::Busy);
            if !session.capabilities.steer {
                return Err(ImmediateSendError::Delivery(
                    crate::DeliveryError::Unsupported(format!(
                        "session {session_id} runtime does not support steer"
                    )),
                ));
            }
            let delivery = self
                .turns
                .submit_steer(
                    session_id,
                    input.clone(),
                    input.metadata.clone(),
                    &turn.turn_id,
                    None,
                )
                .await
                .map_err(ImmediateSendError::Control)?;
            let delivery = immediate_result(delivery.1);
            match delivery {
                Ok(_) => return Ok(SendOutcome::Delivered),
                Err(ImmediateSendError::Control(Error::StateConflict(_)))
                | Err(ImmediateSendError::Control(Error::Conflict { .. }))
                | Err(ImmediateSendError::Delivery(crate::DeliveryError::StateChanged(_))) => {
                    let current = SqliteTurnRepository::new(self.pool.clone())
                        .active_turn(session_id)
                        .await
                        .map_err(ImmediateSendError::Control)?;
                    let session = self
                        .queries
                        .get_session_control(session_id)
                        .await
                        .map_err(ImmediateSendError::Control)?
                        .ok_or_else(|| {
                            ImmediateSendError::Delivery(crate::DeliveryError::StateChanged(
                                format!("session {session_id} not found"),
                            ))
                        })?;
                    if current.is_some()
                        || !matches!(session.state.as_str(), "idle" | "interrupted")
                    {
                        return Err(ImmediateSendError::Delivery(
                            crate::DeliveryError::StateChanged(
                                "session is no longer ready for immediate delivery".into(),
                            ),
                        ));
                    }
                }
                Err(error) => return Err(error),
            }
        } else if matches!(session.state.as_str(), "busy" | "waiting_for_interaction") {
            actor.set_phase(SessionPhase::Busy);
            return Err(ImmediateSendError::Delivery(
                crate::DeliveryError::StateChanged(format!(
                    "session {session_id} is busy without an active turn"
                )),
            ));
        }

        actor.set_phase(SessionPhase::StartingTurn);
        let delivery = match self
            .turns
            .submit_start_turn(
                session_id,
                input.clone(),
                input.metadata.clone(),
                None,
                None,
            )
            .await
        {
            Ok(delivery) => delivery,
            Err(error) => {
                actor.cancel_start();
                return Err(ImmediateSendError::Control(error));
            }
        };
        let delivery = immediate_result(delivery.1);
        match delivery {
            Ok(_) => {
                actor.finish_start();
                Ok(SendOutcome::Delivered)
            }
            Err(error) => {
                actor.cancel_start();
                Err(error)
            }
        }
    }

    async fn queue_input_locked(
        &self,
        session_id: &str,
        mut input: InputPayload,
        delivery_policy: &str,
        interrupt_before_delivery: bool,
        actor: &SessionActor,
    ) -> std::result::Result<SendOutcome, crate::DeliveryError> {
        let repository = SqliteInboxRepository::new(self.pool.clone());
        let force_queue = input.force_queue;
        if !input.force_queue
            && !repository
                .has_unsettled_input(session_id)
                .await
                .map_err(crate::DeliveryError::from)?
        {
            let session = self
                .queries
                .get_session_control(session_id)
                .await
                .map_err(crate::DeliveryError::from)?
                .ok_or_else(|| {
                    crate::DeliveryError::StateChanged(format!("session {session_id} not found"))
                })?;
            let adapter = self
                .clients
                .for_client(&session.client_type)
                .map_err(crate::DeliveryError::from)?;
            let active = SqliteTurnRepository::new(self.pool.clone())
                .active_turn(session_id)
                .await
                .map_err(crate::DeliveryError::from)?;
            let prepared_matches = input.prepared_turn_id.as_deref()
                == active.as_ref().map(|turn| turn.turn_id.as_str());
            let can_start = prepared_matches
                && (input.prepared_turn_id.is_some() || active.is_none())
                && (matches!(session.state.as_str(), "idle" | "interrupted")
                    || (matches!(session.state.as_str(), "created" | "starting")
                        && adapter.prepares_on_input()));
            if can_start
                && adapter
                    .input_available(session_id)
                    .await
                    .map_err(crate::DeliveryError::from)?
            {
                actor.set_phase(SessionPhase::StartingTurn);
                let (_, result) = match self
                    .turns
                    .submit_start_turn(
                        session_id,
                        input.clone(),
                        input.metadata.clone(),
                        None,
                        input.prepared_turn_id.as_deref(),
                    )
                    .await
                {
                    Ok(delivery) => delivery,
                    Err(error) => {
                        actor.cancel_start();
                        return Err(crate::DeliveryError::from(error));
                    }
                };
                match result.into_delivery_result() {
                    Ok(_) => {
                        actor.finish_start();
                        return Ok(SendOutcome::Delivered);
                    }
                    Err(crate::DeliveryError::Uncertain(reason)) => {
                        return Err(crate::DeliveryError::Uncertain(reason));
                    }
                    Err(crate::DeliveryError::Unsupported(reason)) => {
                        return Err(crate::DeliveryError::Unsupported(reason));
                    }
                    Err(
                        crate::DeliveryError::Offline(_) | crate::DeliveryError::StateChanged(_),
                    ) => {
                        input.prepared_turn_id = None;
                        actor.cancel_start();
                    }
                }
            }
        }
        let message_id = new_message_id().to_string();
        let prepared_turn_id = input.prepared_turn_id.clone();
        self.enqueue_message_locked(
            &message_id,
            session_id,
            SubmitInboxMessageRequest {
                input: input.text,
                delivery_policy: delivery_policy.into(),
                metadata: input.metadata,
            },
            EnqueueOptions {
                retry_of: None,
                resuming: false,
                interrupt_before_delivery,
                drain_now: false,
            },
            actor,
        )
        .await
        .map_err(crate::DeliveryError::from)?;
        if let Some(turn_id) = prepared_turn_id {
            SqliteInboxRepository::new(self.pool.clone())
                .attach_prepared_turn(&message_id, &turn_id)
                .await
                .map_err(crate::DeliveryError::from)?;
        }
        if force_queue {
            self.drain_inbox_locked(session_id, actor)
                .await
                .map_err(crate::DeliveryError::from)?;
        }
        self.notify_available(session_id);
        let message = self
            .get_message(session_id, &message_id)
            .await
            .map_err(crate::DeliveryError::from)?
            .ok_or_else(|| {
                crate::DeliveryError::StateChanged("queued message is missing".into())
            })?;
        match message.state.as_str() {
            "dispatched" => Ok(SendOutcome::Delivered),
            "pending" | "dispatching" => Ok(SendOutcome::Queued { message_id }),
            "unknown" => Err(crate::DeliveryError::Uncertain(
                message
                    .failure_message
                    .unwrap_or_else(|| "delivery was not confirmed".into()),
            )),
            _ => Err(crate::DeliveryError::Offline(
                message
                    .failure_message
                    .unwrap_or_else(|| "queued delivery failed".into()),
            )),
        }
    }

    async fn interrupt_input_locked(
        &self,
        session_id: &str,
        input: InputPayload,
        actor: &SessionActor,
    ) -> std::result::Result<SendOutcome, crate::DeliveryError> {
        let active = SqliteTurnRepository::new(self.pool.clone())
            .active_turn(session_id)
            .await
            .map_err(crate::DeliveryError::from)?;
        let session = self
            .queries
            .get_session_control(session_id)
            .await
            .map_err(crate::DeliveryError::from)?
            .ok_or_else(|| {
                crate::DeliveryError::StateChanged(format!("session {session_id} not found"))
            })?;
        if active.is_none() && !matches!(session.state.as_str(), "busy" | "waiting_for_interaction")
        {
            return self
                .deliver_immediate_locked(session_id, input, actor)
                .await
                .map_err(ImmediateSendError::into_delivery);
        }
        actor.set_phase(SessionPhase::Interrupting);
        match self
            .turns
            .interrupt_current_turn_locked(session_id, actor)
            .await
        {
            Ok(_) => {
                self.queue_input_locked(session_id, input, "interrupt_now", false, actor)
                    .await
            }
            Err(error) => match crate::DeliveryError::from(error) {
                crate::DeliveryError::StateChanged(_) => {
                    let active = SqliteTurnRepository::new(self.pool.clone())
                        .active_turn(session_id)
                        .await
                        .map_err(crate::DeliveryError::from)?;
                    let session = self
                        .queries
                        .get_session_control(session_id)
                        .await
                        .map_err(crate::DeliveryError::from)?
                        .ok_or_else(|| {
                            crate::DeliveryError::StateChanged(format!(
                                "session {session_id} not found"
                            ))
                        })?;
                    if active.is_some() || !matches!(session.state.as_str(), "idle" | "interrupted")
                    {
                        return Err(crate::DeliveryError::StateChanged(
                            "session is not idle after interrupt state changed".into(),
                        ));
                    }
                    SqliteInboxRepository::new(self.pool.clone())
                        .supersede_pending_interrupts(session_id)
                        .await
                        .map_err(crate::DeliveryError::from)?;
                    self.deliver_immediate_locked(session_id, input, actor)
                        .await
                        .map_err(ImmediateSendError::into_delivery)
                }
                error => Err(error),
            },
        }
    }
}
