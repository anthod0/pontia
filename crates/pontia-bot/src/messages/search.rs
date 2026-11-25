use pontia_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{
    BotMessage, MESSAGE_COLUMNS, MessageService, TimedMessage,
    attachments::{Attachment, MediaKind, attachments},
};

const MAX_QUERY_CHARACTERS: usize = 200;
/// Candidate messages read per query while looking for file and image matches.
const ATTACHMENT_BATCH: u32 = 200;

/// What a search matches: message text, or the files or images messages refer to.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchType {
    #[default]
    All,
    File,
    Image,
}

#[derive(Serialize)]
pub struct SearchHit {
    #[serde(flatten)]
    pub message: BotMessage,
    /// The matching files of the message, for file and image searches.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<Attachment>>,
}

#[derive(Serialize)]
pub struct SearchPage {
    pub messages: Vec<SearchHit>,
    /// The last returned message ID, when older matches remain.
    pub next_before: Option<String>,
}

impl MessageService {
    /// Messages matching `query`, newest first. ASCII letters match case-insensitively.
    /// Text searches match message content and need a query; file and image searches
    /// match file names and paths, and list every file or image without a query.
    pub(crate) async fn search(
        &self,
        bot_id: &str,
        query: &str,
        kind: SearchType,
        before: Option<&str>,
        limit: u32,
    ) -> Result<SearchPage> {
        let query = query.trim();
        if query.is_empty() && kind == SearchType::All {
            return Err(Error::Domain("search text cannot be empty".into()));
        }
        if query.chars().count() > MAX_QUERY_CHARACTERS {
            return Err(Error::Domain(format!(
                "search text must be at most {MAX_QUERY_CHARACTERS} characters"
            )));
        }
        if !(1..=100).contains(&limit) {
            return Err(Error::Domain("limit must be between 1 and 100".into()));
        }
        let mut cursor = match before {
            Some(before) => {
                let anchor: Option<(i64,)> = sqlx::query_as(
                    "SELECT timeline_ms FROM bot_messages WHERE bot_id = ? AND id = ?",
                )
                .bind(bot_id)
                .bind(before)
                .fetch_optional(&self.pool)
                .await?;
                let (ms,) = anchor.ok_or_else(|| Error::Domain("Invalid search cursor".into()))?;
                Some((ms, before.to_owned()))
            }
            None => None,
        };
        let media = match kind {
            SearchType::All => None,
            SearchType::File => Some(MediaKind::File),
            SearchType::Image => Some(MediaKind::Image),
        };
        let needle = query.to_ascii_lowercase();
        let mut hits = Vec::new();
        loop {
            let batch = if media.is_some() {
                ATTACHMENT_BATCH
            } else {
                limit + 1
            };
            let rows = self
                .candidates(bot_id, query, media.is_some(), cursor.as_ref(), batch)
                .await?;
            let exhausted = rows.len() < batch as usize;
            if let Some(last) = rows.last() {
                cursor = Some((last.timeline_ms, last.message.id.clone()));
            }
            for TimedMessage { message, .. } in rows {
                let attachments = match media {
                    None => None,
                    Some(media) => {
                        let found: Vec<_> = attachments(message.kind, &message.content)
                            .into_iter()
                            .filter(|file| {
                                file.media_kind == media
                                    && (file.name.to_ascii_lowercase().contains(&needle)
                                        || file.path.to_ascii_lowercase().contains(&needle))
                            })
                            .collect();
                        if found.is_empty() {
                            continue;
                        }
                        Some(found)
                    }
                };
                hits.push(SearchHit {
                    message,
                    attachments,
                });
                if hits.len() > limit as usize {
                    break;
                }
            }
            if exhausted || hits.len() > limit as usize {
                break;
            }
        }
        let more = hits.len() > limit as usize;
        hits.truncate(limit as usize);
        let next_before = more.then(|| hits.last().unwrap().message.id.clone());
        Ok(SearchPage {
            messages: hits,
            next_before,
        })
    }

    /// Messages older than `cursor` whose content contains `query`, or for file searches,
    /// that may refer to a file whose name or path contains it.
    async fn candidates(
        &self,
        bot_id: &str,
        query: &str,
        files: bool,
        cursor: Option<&(i64, String)>,
        limit: u32,
    ) -> Result<Vec<TimedMessage>> {
        let mut sql = sqlx::QueryBuilder::new(format!(
            "SELECT {MESSAGE_COLUMNS} FROM bot_messages m WHERE m.bot_id = "
        ));
        sql.push_bind(bot_id);
        if files {
            sql.push(
                " AND ((m.kind = 'user_input' AND m.content LIKE '%<user_upload path=%')
                  OR (m.kind = 'bot_reply' AND (m.content LIKE '%```%attachments%'
                      OR m.content LIKE '%](/%' OR m.content LIKE '%](</%')))",
            );
        }
        // Stored file paths and names can be XML-escaped or percent-encoded, so for file
        // searches the content filter only narrows by text that is never encoded.
        if !files
            || !query.is_empty()
                && query.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(character, '-' | '.' | '_' | '/' | '~')
                })
        {
            let pattern = format!(
                "%{}%",
                query
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            );
            sql.push(" AND m.content LIKE ")
                .push_bind(pattern)
                .push(" ESCAPE '\\'");
        }
        if let Some((ms, id)) = cursor {
            sql.push(" AND (m.timeline_ms, m.id) < (")
                .push_bind(*ms)
                .push(", ")
                .push_bind(id.clone())
                .push(")");
        }
        sql.push(" ORDER BY m.timeline_ms DESC, m.id DESC LIMIT ")
            .push_bind(limit);
        Ok(sql.build_query_as().fetch_all(&self.pool).await?)
    }
}
