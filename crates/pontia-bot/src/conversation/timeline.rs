use pontia_agent_control::queries::{
    OwnedFileShare, OwnedSession, SessionEventSummary, SharedFile, TimePage,
};
use pontia_core::{Error, Result, domain::EventType};
use serde::{Deserialize, Serialize};

use super::{APPLICATION_ID, ConversationService, Notice};
use crate::{
    messages::{BotMessage, TimedMessage},
    records::Bot,
};

pub const NOTICE_EVENTS: &[EventType] = &[
    EventType::TurnFailed,
    EventType::SessionError,
    EventType::SessionExited,
];

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum TimelineEntry {
    /// One `pontia send-file` call; `id` is its share ID.
    #[serde(rename = "shared_files")]
    SharedFiles {
        id: String,
        session_id: String,
        created_at: String,
        files: Vec<SharedFile>,
    },
    #[serde(untagged)]
    Message(BotMessage),
    #[serde(untagged)]
    Notice(Notice),
}

struct TimedEntry {
    ms: i64,
    entry: TimelineEntry,
}

impl TimedEntry {
    fn key(&self) -> (i64, &str) {
        let id = match &self.entry {
            TimelineEntry::SharedFiles { id, .. } => id,
            TimelineEntry::Message(m) => &m.id,
            TimelineEntry::Notice(n) => &n.id,
        };
        (self.ms, id)
    }
}

impl From<TimedMessage> for TimedEntry {
    fn from(row: TimedMessage) -> Self {
        Self {
            ms: row.timeline_ms,
            entry: TimelineEntry::Message(row.message),
        }
    }
}

impl From<OwnedFileShare> for TimedEntry {
    fn from(row: OwnedFileShare) -> Self {
        Self {
            ms: row.timeline_ms,
            entry: TimelineEntry::SharedFiles {
                id: row.share_id,
                session_id: row.session_id,
                created_at: row.created_at,
                files: row.files,
            },
        }
    }
}

impl From<SessionEventSummary> for TimedEntry {
    fn from(row: SessionEventSummary) -> Self {
        Self {
            ms: row.timeline_ms,
            entry: TimelineEntry::Notice(Notice {
                id: row.event_id,
                kind: row.event_type,
                created_at: row.occurred_at,
            }),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    bot: String,
    session: Option<String>,
    ms: i64,
    id: String,
}

#[derive(Serialize)]
pub struct TimelinePage {
    bot: Bot,
    status: super::WorkStatus,
    generation_notice: Option<Notice>,
    notice_session_id: Option<String>,
    file_link_session: Option<OwnedSession>,
    entries: Vec<TimelineEntry>,
    open_todo_count: i64,
    next_before: Option<String>,
}

impl ConversationService {
    pub async fn timeline(
        &self,
        bot_id: &str,
        before: Option<&str>,
        around: Option<&str>,
        limit: u32,
    ) -> Result<TimelinePage> {
        if !(1..=100).contains(&limit) {
            return Err(Error::Domain("limit must be between 1 and 100".into()));
        }
        if before.is_some() && around.is_some() {
            return Err(Error::Domain("Choose either before or around".into()));
        }
        let activity = self.activity(bot_id);
        let _reply = activity.reply.lock().await;
        let _recording = activity.recording.lock().await;
        let bot = self.records.get(bot_id).await?;
        let (rows, more) = if let Some(anchor_id) = around {
            self.around(&bot, anchor_id, limit).await?
        } else {
            let cursor = before
                .map(|value| -> Result<Cursor> {
                    let invalid = || Error::Domain("Invalid Bot timeline cursor".into());
                    let cursor: Cursor = serde_json::from_str(value).map_err(|_| invalid())?;
                    if cursor.bot != bot_id || cursor.id.is_empty() {
                        return Err(invalid());
                    }
                    if cursor.session != bot.current_session_id {
                        return Err(Error::Conflict {
                            code: "bot_timeline_reset",
                            message: "The Bot Session has changed".into(),
                        });
                    }
                    Ok(cursor)
                })
                .transpose()?;
            let bound = cursor
                .as_ref()
                .map_or(TimePage::Latest, |c| TimePage::Before(c.ms, &c.id));
            let mut rows = self.timeline_rows(&bot, bound, limit + 1).await?;
            let more = rows.len() > limit as usize;
            rows.truncate(limit as usize);
            rows.reverse();
            (rows, more)
        };
        let next_before = if more {
            let (ms, id) = rows[0].key();
            Some(serde_json::to_string(&Cursor {
                bot: bot_id.into(),
                session: bot.current_session_id.clone(),
                ms,
                id: id.into(),
            })?)
        } else {
            None
        };
        let status = self.work_status(&bot, &activity).await?;
        let generation_notice = activity.generation_notice.lock().unwrap().clone();
        Ok(TimelinePage {
            open_todo_count: self.messages.open_todo_count(bot_id).await?,
            notice_session_id: bot.current_session_id.clone(),
            file_link_session: self
                .control
                .queries()
                .latest_owned_session(APPLICATION_ID, &format!("{bot_id}:"))
                .await?,
            bot,
            status,
            generation_notice,
            entries: rows.into_iter().map(|row| row.entry).collect(),
            next_before,
        })
    }

    /// Entries around a message, notice, or file share. A file ID locates its share.
    async fn around(&self, bot: &Bot, id: &str, limit: u32) -> Result<(Vec<TimedEntry>, bool)> {
        let anchor = if let Some(row) = self.messages.find(&bot.bot_id, id).await? {
            Some(TimedEntry::from(row))
        } else if let Some(share) = self
            .control
            .queries()
            .owned_file_share(APPLICATION_ID, &format!("{}:", bot.bot_id), id)
            .await?
        {
            Some(TimedEntry::from(share))
        } else if let Some(session) = &bot.current_session_id {
            self.control
                .queries()
                .session_event_summary(session, NOTICE_EVENTS, id)
                .await?
                .map(TimedEntry::from)
        } else {
            None
        }
        .ok_or_else(|| Error::NotFound("Bot timeline entry not found".into()))?;

        let (ms, id) = anchor.key();
        let id = id.to_owned();
        let mut older = self
            .timeline_rows(bot, TimePage::Before(ms, &id), limit)
            .await?;
        let mut newer = self
            .timeline_rows(bot, TimePage::After(ms, &id), limit - 1)
            .await?;
        let neighbors = limit as usize - 1;
        let older_count = (neighbors / 2)
            .max(neighbors - newer.len())
            .min(older.len());
        let more = older.len() > older_count;
        older.truncate(older_count);
        older.reverse();
        newer.truncate(neighbors - older_count);
        older.push(anchor);
        older.extend(newer);
        Ok((older, more))
    }

    async fn timeline_rows(
        &self,
        bot: &Bot,
        page: TimePage<'_>,
        limit: u32,
    ) -> Result<Vec<TimedEntry>> {
        let mut rows: Vec<_> = self
            .messages
            .page(&bot.bot_id, page, limit)
            .await?
            .into_iter()
            .map(TimedEntry::from)
            .collect();
        if let Some(session) = &bot.current_session_id {
            let notices = self
                .control
                .queries()
                .session_event_page(session, NOTICE_EVENTS, page, limit)
                .await?;
            rows.extend(notices.into_iter().map(TimedEntry::from));
        }
        let shares = self
            .control
            .queries()
            .owned_file_shares(APPLICATION_ID, &format!("{}:", bot.bot_id), page, limit)
            .await?;
        rows.extend(shares.into_iter().map(TimedEntry::from));
        rows.sort_unstable_by(|a, b| a.key().cmp(&b.key()));
        if !matches!(page, TimePage::After(..)) {
            rows.reverse();
        }
        rows.truncate(limit as usize);
        Ok(rows)
    }
}
