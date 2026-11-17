use std::sync::Arc;

use sqlx::SqlitePool;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::{keywords::Keywords, storage::Store, *};

#[derive(Clone)]
pub struct MemoryService {
    store: Store,
    clock: Arc<dyn Fn() -> OffsetDateTime + Send + Sync>,
}

impl MemoryService {
    pub fn new(pool: SqlitePool) -> Self {
        Self::with_clock(pool, OffsetDateTime::now_utc)
    }

    pub fn with_clock(
        pool: SqlitePool,
        clock: impl Fn() -> OffsetDateTime + Send + Sync + 'static,
    ) -> Self {
        Self {
            store: Store(pool),
            clock: Arc::new(clock),
        }
    }

    pub async fn execute(
        &self,
        access: &MemoryAccess,
        command: MemoryCommand,
    ) -> Result<MemoryResponse> {
        use MemoryCommand::*;
        let allowed = match &command {
            Create(_) => matches!(access.role, MemoryRole::Bot | MemoryRole::Dreaming),
            Use(_) => access.role == MemoryRole::Bot,
            Update(_) | Archive(_) | Link(_) | Unlink(_) => access.role == MemoryRole::Dreaming,
            Read(_) | Search(_) | Neighbors(_) => true,
        };
        if !allowed {
            return Err(Error::Forbidden);
        }
        let now = (self.clock)().to_offset(time::UtcOffset::UTC);
        match command {
            Create(mut input) => {
                validate_content(&input.content, input.sources.as_deref())?;
                validate_optional_id(input.workspace_id.as_deref())?;
                validate_optional_id(input.session_id.as_deref())?;
                // A scoped writer must explicitly place new memories in its assigned Workspace.
                if access.workspace_id.is_some() && access.workspace_id != input.workspace_id {
                    return Err(Error::Forbidden);
                }
                input.occurred_at = occurred_at(input.occurred_at)?;
                let keywords = Keywords::parse(std::mem::take(&mut input.keywords))?;
                let author = if access.role == MemoryRole::Bot {
                    MemoryAuthor::Bot
                } else {
                    MemoryAuthor::Dreaming
                };
                let memory = self
                    .store
                    .create(input, keywords, author, &now.format(&Rfc3339)?)
                    .await?;
                Ok(MemoryResponse::Created { memory })
            }
            Update(mut input) => {
                validate_id(&input.id)?;
                validate_content(&input.content, input.sources.as_deref())?;
                input.occurred_at = occurred_at(input.occurred_at)?;
                let keywords = Keywords::parse(std::mem::take(&mut input.keywords))?;
                self.store
                    .update(
                        input,
                        keywords,
                        access.workspace_id.as_deref(),
                        &now.format(&Rfc3339)?,
                    )
                    .await?;
                Ok(MemoryResponse::Changed { affected: 1 })
            }
            Archive(input) => {
                validate_id(&input.id)?;
                self.store
                    .archive(
                        input,
                        access.workspace_id.as_deref(),
                        &now.format(&Rfc3339)?,
                    )
                    .await?;
                Ok(MemoryResponse::Changed { affected: 1 })
            }
            Link(input) => self.edge(input, true, access, now).await,
            Unlink(input) => self.edge(input, false, access, now).await,
            Read(input) => {
                validate_ids(&input.ids)?;
                let workspace = scope(access, input.workspace_id.as_deref())?;
                Ok(MemoryResponse::Read {
                    memories: self.store.read(&input.ids, workspace).await?,
                })
            }
            Use(input) => {
                validate_ids(&input.ids)?;
                let workspace = scope(access, input.workspace_id.as_deref())?;
                let affected = self
                    .store
                    .feedback(&input.ids, workspace, &now.format(&Rfc3339)?)
                    .await?;
                Ok(MemoryResponse::Changed { affected })
            }
            Search(input) => {
                let limit = limit(input.limit, 10, 30)?;
                let workspace = scope(access, input.workspace_id.as_deref())?;
                let keywords = Keywords::parse(input.keywords)?;
                let candidates = self.store.candidates(&keywords, workspace).await?;
                let mut results = Vec::with_capacity(candidates.len());
                for (memory, relevance) in candidates {
                    let base = OffsetDateTime::parse(
                        memory.last_read_at.as_deref().unwrap_or(&memory.created_at),
                        &Rfc3339,
                    )?;
                    let days = (now - base).as_seconds_f64().max(0.0) / 86_400.0;
                    let freshness = 2_f64.powf(-days / 6.0);
                    let importance = match memory.importance {
                        Importance::Low => 0.0,
                        Importance::Normal => 0.5,
                        Importance::High => 1.0,
                    };
                    let score = relevance * (1.0 + (2.0 / 3.0) * importance + freshness / 6.0);
                    results.push(MemoryHit {
                        memory,
                        relevance,
                        score,
                    });
                }
                results.sort_by(|a, b| {
                    b.score
                        .total_cmp(&a.score)
                        .then_with(|| a.memory.id.cmp(&b.memory.id))
                });
                let has_more = results.len() > limit;
                results.truncate(limit);
                Ok(MemoryResponse::Search { results, has_more })
            }
            Neighbors(input) => {
                validate_id(&input.id)?;
                validate_optional_id(input.after.as_deref())?;
                let limit = limit(input.limit, 30, 100)?;
                let workspace = scope(access, input.workspace_id.as_deref())?;
                let mut memories = self
                    .store
                    .neighbors(&input.id, workspace, input.after.as_deref(), limit)
                    .await?;
                let has_more = memories.len() > limit;
                memories.truncate(limit);
                let next_cursor = if has_more {
                    memories.last().map(|m| m.id.clone())
                } else {
                    None
                };
                Ok(MemoryResponse::Neighbors {
                    memories,
                    next_cursor,
                })
            }
        }
    }

    async fn edge(
        &self,
        input: MemoryEdge,
        link: bool,
        access: &MemoryAccess,
        now: OffsetDateTime,
    ) -> Result<MemoryResponse> {
        validate_id(&input.memory_a_id)?;
        validate_id(&input.memory_b_id)?;
        if input.memory_a_id == input.memory_b_id {
            return Err(Error::InvalidInput("a memory cannot link to itself".into()));
        }
        let (a, b) = if input.memory_a_id < input.memory_b_id {
            (input.memory_a_id, input.memory_b_id)
        } else {
            (input.memory_b_id, input.memory_a_id)
        };
        let affected = self
            .store
            .edge(
                &a,
                &b,
                link,
                access.workspace_id.as_deref(),
                &now.format(&Rfc3339)?,
            )
            .await?;
        Ok(MemoryResponse::Changed { affected })
    }
}

fn scope<'a>(access: &'a MemoryAccess, requested: Option<&'a str>) -> Result<Option<&'a str>> {
    validate_optional_id(requested)?;
    match (access.workspace_id.as_deref(), requested) {
        (Some(bound), Some(requested)) if bound != requested => Err(Error::Forbidden),
        (Some(bound), _) => Ok(Some(bound)),
        (None, requested) => Ok(requested),
    }
}

fn validate_id(id: &str) -> Result<()> {
    if id.trim().is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
        return Err(Error::InvalidInput(
            "IDs must contain 1–256 bytes without control characters".into(),
        ));
    }
    Ok(())
}

fn validate_optional_id(id: Option<&str>) -> Result<()> {
    if let Some(id) = id {
        validate_id(id)?;
    }
    Ok(())
}

fn validate_ids(ids: &[String]) -> Result<()> {
    if ids.is_empty() || ids.len() > 100 {
        return Err(Error::InvalidInput("ids must contain 1–100 IDs".into()));
    }
    for id in ids {
        validate_id(id)?;
    }
    Ok(())
}

fn limit(input: Option<usize>, default: usize, max: usize) -> Result<usize> {
    let limit = input.unwrap_or(default);
    if !(1..=max).contains(&limit) {
        return Err(Error::InvalidInput(format!(
            "limit must be between 1 and {max}"
        )));
    }
    Ok(limit)
}

fn validate_content(content: &str, sources: Option<&str>) -> Result<()> {
    if content.trim().is_empty() || content.len() > 65_536 || content.contains('\0') {
        return Err(Error::InvalidInput(
            "content must be nonblank, at most 65536 bytes and contain no NUL".into(),
        ));
    }
    if sources.is_some_and(|s| s.len() > 16_384 || s.contains('\0')) {
        return Err(Error::InvalidInput(
            "sources must be at most 16384 bytes and contain no NUL".into(),
        ));
    }
    Ok(())
}

fn occurred_at(input: Option<String>) -> Result<Option<String>> {
    input
        .map(|input| {
            let time = OffsetDateTime::parse(&input, &Rfc3339).map_err(|_| {
                Error::InvalidInput("occurred_at must be an RFC 3339 timestamp".into())
            })?;
            Ok(time.to_offset(time::UtcOffset::UTC).format(&Rfc3339)?)
        })
        .transpose()
}
