use crate::conversation::ConversationService;
use pontia_agent_control::{AgentControl, ControlChange};
use std::{collections::HashMap, future::pending, time::Duration};
use tokio::{
    sync::{broadcast, oneshot, watch},
    task::JoinSet,
    time::{Instant, sleep_until},
};

const WINDOW: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub(crate) struct BotNotice {
    /// `None` addresses every Bot with a live Session.
    pub bot_id: Option<String>,
    pub key: String,
    pub text: String,
}

#[derive(Clone)]
struct Batch {
    deadline: Instant,
    entries: Vec<(String, String)>,
}

impl Batch {
    fn add(batch: &mut Option<Self>, key: &str, text: &str, now: Instant) {
        let batch = batch.get_or_insert_with(|| Self {
            deadline: now + WINDOW,
            entries: Vec::new(),
        });
        batch.entries.retain(|(previous, _)| previous != key);
        batch.entries.push((key.into(), text.into()));
    }

    fn input(self) -> String {
        let body = self
            .entries
            .into_iter()
            .map(|(_, text)| text)
            .collect::<Vec<_>>()
            .join("\n\n");
        format!("Source: System Notification\n{body}")
    }
}

#[derive(Default)]
struct Windows {
    // Bots share a batch until a targeted change gives them different contents or deadlines.
    shared: Option<Batch>,
    individual: HashMap<String, Option<Batch>>,
}

impl Windows {
    fn individual(&mut self, bot: &str) -> &mut Option<Batch> {
        self.individual
            .entry(bot.into())
            .or_insert_with(|| self.shared.clone())
    }

    fn broadcast(&mut self, key: &str, text: &str, except: Option<&str>) {
        let now = Instant::now();
        if let Some(bot) = except {
            self.individual(bot);
        }
        Batch::add(&mut self.shared, key, text, now);
        for (bot, batch) in &mut self.individual {
            if Some(bot.as_str()) != except {
                Batch::add(batch, key, text, now);
            }
        }
    }

    fn deadline(&self) -> Option<Instant> {
        self.shared
            .iter()
            .chain(self.individual.values().flatten())
            .map(|batch| batch.deadline)
            .min()
    }

    async fn flush(&mut self, conversation: &ConversationService, deliveries: &mut Deliveries) {
        let now = Instant::now();
        deliveries
            .tails
            .retain(|_, tail| matches!(tail.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
        if self
            .shared
            .as_ref()
            .is_some_and(|batch| batch.deadline <= now)
        {
            let input = self.shared.take().unwrap().input();
            match conversation.records().list().await {
                Ok(bots) => {
                    for bot in bots {
                        if !self.individual.contains_key(&bot.bot_id)
                            && bot.current_session_id.is_some()
                        {
                            deliveries.send(conversation, bot.bot_id, input.clone());
                        }
                    }
                }
                Err(error) => {
                    tracing::debug!(%error, "Dropped Bot system notices during recipient lookup")
                }
            }
        }
        for (bot, batch) in &mut self.individual {
            if batch.as_ref().is_some_and(|b| b.deadline <= now) {
                deliveries.send(conversation, bot.clone(), batch.take().unwrap().input());
            }
        }
        if self.shared.is_none() {
            self.individual.retain(|_, batch| batch.is_some());
        }
    }
}

#[derive(Default)]
struct Deliveries {
    tasks: JoinSet<()>,
    // Each delivery waits only for its own Bot's preceding batch.
    tails: HashMap<String, oneshot::Receiver<()>>,
}

impl Deliveries {
    fn send(&mut self, conversation: &ConversationService, bot: String, input: String) {
        let (finished, tail) = oneshot::channel();
        let previous = self.tails.insert(bot.clone(), tail);
        let conversation = conversation.clone();
        self.tasks.spawn(async move {
            if let Some(previous) = previous {
                let _ = previous.await;
            }
            if let Err(error) = conversation.deliver_system_notice(&bot, &input).await {
                tracing::debug!(bot_id = %bot, %error, "Dropped Bot system notice");
            }
            let _ = finished.send(());
        });
    }
}

pub struct SystemNoticeService {
    conversation: ConversationService,
    changes: broadcast::Receiver<ControlChange>,
    notices: broadcast::Receiver<BotNotice>,
}

impl SystemNoticeService {
    pub(crate) fn new(
        conversation: ConversationService,
        control: AgentControl,
        notices: broadcast::Receiver<BotNotice>,
    ) -> Self {
        Self {
            conversation,
            changes: control.control_changes().subscribe(),
            notices,
        }
    }

    pub async fn run(mut self, mut shutdown: watch::Receiver<bool>) {
        use broadcast::error::RecvError;
        let mut windows = Windows::default();
        let mut deliveries = Deliveries::default();
        while !*shutdown.borrow() {
            let deadline = windows.deadline();
            if deadline.is_some_and(|at| at <= Instant::now()) {
                windows.flush(&self.conversation, &mut deliveries).await;
                continue;
            }
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = async { match deadline { Some(at) => sleep_until(at).await, None => pending().await } } => {
                }
                Some(result) = deliveries.tasks.join_next(), if !deliveries.tasks.is_empty() => {
                    if let Err(error) = result { tracing::warn!(%error, "Bot system notice task failed"); }
                }
                change = self.changes.recv() => match change {
                    Ok(change) => self.control_change(change, &mut windows).await,
                    Err(RecvError::Lagged(skipped)) => tracing::warn!(skipped, "Missed control changes"),
                    Err(RecvError::Closed) => break,
                },
                notice = self.notices.recv() => match notice {
                    Ok(BotNotice { bot_id: Some(bot), key, text }) => Batch::add(windows.individual(&bot), &key, &text, Instant::now()),
                    Ok(BotNotice { bot_id: None, key, text }) => windows.broadcast(&key, &text, None),
                    Err(RecvError::Lagged(skipped)) => tracing::warn!(skipped, "Missed Bot notices"),
                    Err(RecvError::Closed) => break,
                },
            }
        }
    }

    async fn control_change(&self, change: ControlChange, windows: &mut Windows) {
        let (key, text, source_session_id) = match change {
            ControlChange::WorkspaceRegistered {
                workspace_id,
                canonical_path,
                description,
                source_session_id,
                ..
            } => (
                format!("workspace:{workspace_id}"),
                format!(
                    "Workspace registered: {workspace_id}\nPath: {canonical_path}\nDescription: {}",
                    description.unwrap_or_default()
                ),
                source_session_id,
            ),
            ControlChange::WorkspaceRenamed { .. } | ControlChange::WorkspaceDeleted { .. } => {
                return;
            }
            ControlChange::AgentProfileCreated {
                profile_id,
                source_session_id,
                name,
                description,
            } => (
                format!("profile:{profile_id}"),
                format!(
                    "Agent Profile created: {profile_id}\nName: {name}\nDescription: {}",
                    description.unwrap_or_default()
                ),
                source_session_id,
            ),
            ControlChange::AgentProfileDeleted {
                profile_id,
                source_session_id,
            } => (
                format!("profile:{profile_id}"),
                format!("Agent Profile deleted: {profile_id}"),
                source_session_id,
            ),
            ControlChange::AgentProfileUpdated {
                profile_id,
                source_session_id,
                description,
                client_type,
                client_key,
                model,
                reasoning_effort,
            } => {
                let client = client_type
                    .zip(client_key)
                    .map(|(kind, key)| format!("{kind}/{key}"))
                    .unwrap_or_default();
                (
                    format!("profile:{profile_id}"),
                    format!(
                        "Agent Profile updated: {profile_id}\nDescription: {}\nClient: {client}\nModel: {}\nReasoning effort: {}",
                        description.unwrap_or_default(),
                        model.unwrap_or_default(),
                        reasoning_effort.unwrap_or_default()
                    ),
                    source_session_id,
                )
            }
        };
        let except = match source_session_id {
            Some(session) => match self.conversation.owner_bot_id(&session).await {
                Ok(id) => id,
                Err(error) => {
                    tracing::debug!(%error, "Dropped system notice during source lookup");
                    return;
                }
            },
            None => None,
        };
        windows.broadcast(&key, &text, except.as_deref());
    }
}
