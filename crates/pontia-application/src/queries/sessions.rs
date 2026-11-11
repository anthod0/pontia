use std::collections::HashMap;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use pontia_core::{Error, error::Result};
use pontia_storage_sqlite::{
    models::sessions::SessionRow,
    repositories::sessions::{
        SessionOverviewCursor, SessionOverviewOptions, SqliteSessionRepository,
    },
};
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use super::ExternalQueryService;
use pontia_agent_control::SessionView;

const DEFAULT_OVERVIEW_LIMIT: u32 = 50;
const MAX_OVERVIEW_LIMIT: u32 = 200;
const OVERVIEW_CURSOR_VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct SessionOverviewRequest {
    pub sections: Option<String>,
    pub workspace_id: Option<String>,
    pub limit: Option<String>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SessionOverviewView {
    pub groups: SessionOverviewGroupsView,
}

#[derive(Debug, Serialize)]
pub struct SessionOverviewGroupsView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<SessionOverviewGroupView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived: Option<SessionOverviewGroupView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<SessionOverviewGroupView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list: Option<SessionOverviewListGroupView>,
}

#[derive(Debug, Serialize)]
pub struct SessionOverviewGroupView {
    pub sessions: Vec<SessionView>,
}

#[derive(Debug, Serialize)]
pub struct SessionOverviewListGroupView {
    pub sessions: Vec<SessionView>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Default)]
struct RequestedSections {
    pinned: bool,
    archived: bool,
    active: bool,
    list: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionOverviewCursorPayload {
    version: u8,
    workspace_id: Option<String>,
    updated_at: String,
    session_id: String,
}

impl ExternalQueryService {
    pub async fn session_overview(
        &self,
        request: SessionOverviewRequest,
    ) -> Result<SessionOverviewView> {
        let sections = parse_overview_sections(request.sections.as_deref())?;
        let limit = parse_overview_limit(request.limit.as_deref())?;
        let workspace_id = if sections.list {
            request
                .workspace_id
                .map(validate_workspace_id)
                .transpose()?
        } else {
            None
        };
        if request.cursor.is_some() && !sections.list {
            return Err(Error::Domain(
                "cursor can only be used when the list section is requested".into(),
            ));
        }
        let cursor = request
            .cursor
            .as_deref()
            .map(|cursor| decode_overview_cursor(cursor, workspace_id.as_deref()))
            .transpose()?;

        let rows = SqliteSessionRepository::new(self.pool.clone())
            .session_overview(SessionOverviewOptions {
                include_pinned: sections.pinned,
                include_archived: sections.archived,
                include_active: sections.active,
                include_list: sections.list,
                workspace_id: workspace_id.clone(),
                cursor,
                list_limit: limit,
            })
            .await?;
        if !rows.workspace_exists {
            return Err(Error::NotFound(format!(
                "workspace {} not found",
                workspace_id.expect("workspace existence is only checked for a workspace scope")
            )));
        }

        let mut enriched = HashMap::new();
        let pinned = match rows.pinned {
            Some(rows) => Some(SessionOverviewGroupView {
                sessions: self.enrich_overview_rows(rows, &mut enriched).await?,
            }),
            None => None,
        };
        let archived = match rows.archived {
            Some(rows) => Some(SessionOverviewGroupView {
                sessions: self.enrich_overview_rows(rows, &mut enriched).await?,
            }),
            None => None,
        };
        let active = match rows.active {
            Some(rows) => Some(SessionOverviewGroupView {
                sessions: self.enrich_overview_rows(rows, &mut enriched).await?,
            }),
            None => None,
        };
        let list = match rows.list {
            Some(rows) => Some(SessionOverviewListGroupView {
                sessions: self
                    .enrich_overview_rows(rows.sessions, &mut enriched)
                    .await?,
                next_cursor: rows
                    .next_cursor
                    .map(|cursor| encode_overview_cursor(cursor, workspace_id.clone()))
                    .transpose()?,
            }),
            None => None,
        };

        Ok(SessionOverviewView {
            groups: SessionOverviewGroupsView {
                pinned,
                archived,
                active,
                list,
            },
        })
    }

    async fn enrich_overview_rows(
        &self,
        rows: Vec<SessionRow>,
        enriched: &mut HashMap<String, SessionView>,
    ) -> Result<Vec<SessionView>> {
        let mut sessions = Vec::with_capacity(rows.len());
        for row in rows {
            if let Some(session) = enriched.get(&row.session_id) {
                sessions.push(session.clone());
                continue;
            }
            let session = self.control.session_view(row).await?;
            enriched.insert(session.session_id.clone(), session.clone());
            sessions.push(session);
        }
        Ok(sessions)
    }
}

fn parse_overview_sections(value: Option<&str>) -> Result<RequestedSections> {
    let value = value.ok_or_else(|| Error::Domain("sections is required".into()))?;
    if value.is_empty() {
        return Err(Error::Domain("sections must not be empty".into()));
    }

    let mut sections = RequestedSections::default();
    for section in value.split(',') {
        match section {
            "pinned" => sections.pinned = true,
            "archived" => sections.archived = true,
            "active" => sections.active = true,
            "list" => sections.list = true,
            _ => {
                return Err(Error::Domain(format!(
                    "unknown session overview section: {section}"
                )));
            }
        }
    }
    Ok(sections)
}

fn parse_overview_limit(value: Option<&str>) -> Result<u32> {
    let limit = match value {
        Some(value) => value
            .parse::<u32>()
            .map_err(|_| Error::Domain("limit must be an integer from 1 to 200".into()))?,
        None => DEFAULT_OVERVIEW_LIMIT,
    };
    if !(1..=MAX_OVERVIEW_LIMIT).contains(&limit) {
        return Err(Error::Domain("limit must be from 1 to 200".into()));
    }
    Ok(limit)
}

fn validate_workspace_id(workspace_id: String) -> Result<String> {
    let valid = !workspace_id.is_empty()
        && workspace_id.len() <= 255
        && workspace_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if !valid {
        return Err(Error::Domain("workspace_id has an invalid format".into()));
    }
    Ok(workspace_id)
}

fn encode_overview_cursor(
    cursor: SessionOverviewCursor,
    workspace_id: Option<String>,
) -> Result<String> {
    let payload = SessionOverviewCursorPayload {
        version: OVERVIEW_CURSOR_VERSION,
        workspace_id,
        updated_at: cursor.updated_at,
        session_id: cursor.session_id,
    };
    Ok(URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload)?))
}

fn decode_overview_cursor(
    cursor: &str,
    expected_workspace_id: Option<&str>,
) -> Result<SessionOverviewCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| Error::Domain("session overview cursor is invalid".into()))?;
    let payload: SessionOverviewCursorPayload = serde_json::from_slice(&bytes)
        .map_err(|_| Error::Domain("session overview cursor is invalid".into()))?;
    if payload.version != OVERVIEW_CURSOR_VERSION {
        return Err(Error::Domain(
            "session overview cursor version is unsupported".into(),
        ));
    }
    if payload.workspace_id.as_deref() != expected_workspace_id {
        return Err(Error::Domain(
            "session overview cursor scope does not match the request".into(),
        ));
    }
    if payload.session_id.is_empty()
        || OffsetDateTime::parse(&payload.updated_at, &Rfc3339).is_err()
    {
        return Err(Error::Domain("session overview cursor is invalid".into()));
    }
    Ok(SessionOverviewCursor {
        updated_at: payload.updated_at,
        session_id: payload.session_id,
    })
}
