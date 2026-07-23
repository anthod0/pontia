use std::{
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::ExitCode,
    str::FromStr,
};

use pontia_agent_clients::{
    pi::raw_transcripts::{PiAgentBindingResolver, PiJsonlV2Cursor, TimelineBoundaryRelation},
    raw_transcripts::{AgentBindingResolveRequest, AgentBindingResolver},
};
use serde::Serialize;
use serde_json::Value;
use sqlx::{ConnectOptions, Row, SqlitePool, sqlite::SqliteConnectOptions};

#[derive(Debug)]
struct Args {
    database: PathBuf,
    pi_agent_dir: PathBuf,
    apply: bool,
    backup: Option<PathBuf>,
}

#[derive(Debug)]
struct CandidateRow {
    event_id: String,
    session_id: String,
    turn_id: String,
    terminal_leaf_id: Option<String>,
    turn_state: String,
    terminal_event_count: i64,
    head_cursor: Option<String>,
    event_timeline_boundary: Option<String>,
    binding_id: Option<String>,
    binding_client_type: Option<String>,
    launch_cwd: Option<String>,
    client_session_key: Option<String>,
    next_head_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
struct RepairPlan {
    mode: &'static str,
    database: String,
    backup: Option<String>,
    summary: RepairSummary,
    candidates: Vec<RepairCandidate>,
}

#[derive(Debug, Serialize)]
struct RepairSummary {
    candidates: usize,
    repairable: usize,
    blocked: usize,
    applied: usize,
}

#[derive(Debug, Serialize)]
struct RepairCandidate {
    event_id: String,
    session_id: String,
    turn_id: String,
    terminal_leaf_id: Option<String>,
    binding_id: Option<String>,
    source_file: Option<String>,
    proposed_tail_cursor: Option<String>,
    proposed_event_timeline_boundary: Option<Value>,
    status: &'static str,
    errors: Vec<String>,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(2),
        Err(error) => {
            eprintln!("repair dry-run failed: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<usize, Box<dyn std::error::Error>> {
    let args = parse_args()?;
    if args.apply {
        validate_backup(&args.database, args.backup.as_deref())?;
    }
    let options = SqliteConnectOptions::from_str(args.database.to_string_lossy().as_ref())?
        .read_only(!args.apply)
        .create_if_missing(false)
        .disable_statement_logging();
    let pool = SqlitePool::connect_with(options).await?;
    let rows = load_candidates(&pool).await?;
    let resolver = PiAgentBindingResolver::with_agent_dir(args.pi_agent_dir);
    let mut candidates = Vec::with_capacity(rows.len());

    for row in rows {
        candidates.push(build_candidate(row, &resolver));
    }

    let repairable = candidates
        .iter()
        .filter(|candidate| candidate.errors.is_empty())
        .count();
    let blocked = candidates.len() - repairable;
    let applied = if args.apply && blocked == 0 {
        apply_candidates(&pool, &candidates).await?
    } else {
        0
    };
    let plan = RepairPlan {
        mode: if args.apply { "apply" } else { "dry-run" },
        database: args.database.display().to_string(),
        backup: args.backup.map(|path| path.display().to_string()),
        summary: RepairSummary {
            candidates: candidates.len(),
            repairable,
            blocked,
            applied,
        },
        candidates,
    };
    println!("{}", serde_json::to_string_pretty(&plan)?);
    Ok(blocked)
}

fn parse_args() -> Result<Args, String> {
    let mut database = None;
    let mut pi_agent_dir = None;
    let mut apply = false;
    let mut backup = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--database" => database = args.next().map(PathBuf::from),
            "--pi-agent-dir" => pi_agent_dir = args.next().map(PathBuf::from),
            "--apply" => apply = true,
            "--backup" => backup = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                return Err(
                    "usage: repair_interrupted_turn_timelines --database <pontia.db> [--pi-agent-dir <dir>] [--apply --backup <backup.db>]"
                        .to_string(),
                );
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    let database = database.ok_or_else(|| "--database is required".to_string())?;
    let pi_agent_dir = pi_agent_dir.unwrap_or_else(default_pi_agent_dir);
    if !apply && backup.is_some() {
        return Err("--backup requires --apply".to_string());
    }
    Ok(Args {
        database,
        pi_agent_dir,
        apply,
        backup,
    })
}

fn validate_backup(
    database: &Path,
    backup: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let backup = backup.ok_or("--apply requires --backup")?;
    if database.canonicalize()? == backup.canonicalize()? {
        return Err("backup path must differ from the database path".into());
    }
    if backup.metadata()?.len() == 0 {
        return Err("backup file is empty".into());
    }
    Ok(())
}

async fn apply_candidates(
    pool: &SqlitePool,
    candidates: &[RepairCandidate],
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut transaction = pool.begin().await?;
    for candidate in candidates {
        let cursor = candidate
            .proposed_tail_cursor
            .as_deref()
            .ok_or("repairable candidate has no proposed tail cursor")?;
        let timeline_boundary = serde_json::to_string(
            candidate
                .proposed_event_timeline_boundary
                .as_ref()
                .ok_or("repairable candidate has no proposed event boundary")?,
        )?;
        let event_result = sqlx::query(
            "UPDATE events SET timeline_boundary = ? WHERE event_id = ? AND event_type = 'turn.interrupted' AND timeline_boundary IS NULL",
        )
        .bind(timeline_boundary)
        .bind(&candidate.event_id)
        .execute(&mut *transaction)
        .await?;
        if event_result.rows_affected() != 1 {
            return Err(format!(
                "event {} changed after validation; rolling back",
                candidate.event_id
            )
            .into());
        }
        let turn_result = sqlx::query(
            "UPDATE turns SET tail_cursor = ? WHERE turn_id = ? AND session_id = ? AND state = 'interrupted' AND tail_cursor IS NULL",
        )
        .bind(cursor)
        .bind(&candidate.turn_id)
        .bind(&candidate.session_id)
        .execute(&mut *transaction)
        .await?;
        if turn_result.rows_affected() != 1 {
            return Err(format!(
                "turn {} changed after validation; rolling back",
                candidate.turn_id
            )
            .into());
        }
    }
    transaction.commit().await?;
    Ok(candidates.len())
}

fn default_pi_agent_dir() -> PathBuf {
    env::var_os("PI_AGENT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".pi/agent")
        })
}

async fn load_candidates(pool: &SqlitePool) -> Result<Vec<CandidateRow>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT e.event_id,
               e.session_id,
               e.turn_id,
               json_extract(e.payload, '$.terminal_leaf_id') AS terminal_leaf_id,
               t.state AS turn_state,
               (
                   SELECT COUNT(*)
                   FROM events terminal
                   WHERE terminal.turn_id = e.turn_id
                     AND terminal.session_id = e.session_id
                     AND terminal.event_type IN (
                         'turn.completed',
                         'turn.failed',
                         'turn.interrupted',
                         'turn.dispatch_failed',
                         'turn.abandoned'
                     )
               ) AS terminal_event_count,
               t.head_cursor,
               e.timeline_boundary AS event_timeline_boundary,
               a.id AS binding_id,
               a.client_type AS binding_client_type,
               a.launch_cwd,
               a.client_session_key,
               (
                   SELECT later.head_cursor
                   FROM turns later
                   WHERE later.session_id = e.session_id
                     AND later.turn_id > e.turn_id
                     AND later.head_cursor IS NOT NULL
                   ORDER BY later.turn_id
                   LIMIT 1
               ) AS next_head_cursor
        FROM events e
        JOIN turns t ON t.turn_id = e.turn_id AND t.session_id = e.session_id
        LEFT JOIN agent_bindings a ON a.session_id = e.session_id
        WHERE e.event_type = 'turn.interrupted'
          AND t.tail_cursor IS NULL
        ORDER BY e.occurred_at, e.event_id
        "#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| {
            Ok(CandidateRow {
                event_id: row.try_get("event_id")?,
                session_id: row.try_get("session_id")?,
                turn_id: row.try_get("turn_id")?,
                terminal_leaf_id: row.try_get("terminal_leaf_id")?,
                turn_state: row.try_get("turn_state")?,
                terminal_event_count: row.try_get("terminal_event_count")?,
                head_cursor: row.try_get("head_cursor")?,
                event_timeline_boundary: row.try_get("event_timeline_boundary")?,
                binding_id: row.try_get("binding_id")?,
                binding_client_type: row.try_get("binding_client_type")?,
                launch_cwd: row.try_get("launch_cwd")?,
                client_session_key: row.try_get("client_session_key")?,
                next_head_cursor: row.try_get("next_head_cursor")?,
            })
        })
        .collect()
}

fn build_candidate(row: CandidateRow, resolver: &PiAgentBindingResolver) -> RepairCandidate {
    let mut errors = Vec::new();
    let mut source_file = None;
    let mut proposed_tail_cursor = None;

    if row.turn_state != "interrupted" {
        errors.push(format!(
            "turn state is {}, expected interrupted",
            row.turn_state
        ));
    }
    if row.terminal_event_count != 1 {
        errors.push(format!(
            "turn has {} terminal lifecycle events, expected exactly one",
            row.terminal_event_count
        ));
    }
    if row.event_timeline_boundary.is_some() {
        errors.push("interrupted event already has timeline_boundary".to_string());
    }
    let terminal_leaf_id = required(&row.terminal_leaf_id, "terminal_leaf_id", &mut errors);
    let binding_id = required(&row.binding_id, "agent binding", &mut errors);
    let client_type = required(&row.binding_client_type, "binding client_type", &mut errors);
    let launch_cwd = required(&row.launch_cwd, "binding launch_cwd", &mut errors);
    let client_session_key = required(
        &row.client_session_key,
        "binding client_session_key",
        &mut errors,
    );
    if client_type.is_some_and(|value| value != "pi") {
        errors.push("agent binding client_type is not pi".to_string());
    }

    if let (
        Some(terminal_leaf_id),
        Some(binding_id),
        Some(client_type),
        Some(launch_cwd),
        Some(client_session_key),
    ) = (
        terminal_leaf_id,
        binding_id,
        client_type,
        launch_cwd,
        client_session_key,
    ) {
        match resolver.resolve(&AgentBindingResolveRequest {
            id: binding_id.to_string(),
            session_id: row.session_id.clone(),
            client_type: client_type.to_string(),
            launch_cwd: PathBuf::from(launch_cwd),
            client_session_key: client_session_key.to_string(),
        }) {
            Ok(source) => {
                source_file = Some(source.path.display().to_string());
                match locate_entry_line_end(&source.path, terminal_leaf_id) {
                    Ok(offset) => {
                        validate_offset(
                            offset,
                            binding_id,
                            row.head_cursor.as_deref(),
                            row.next_head_cursor.as_deref(),
                            &mut errors,
                        );
                        if errors.is_empty() {
                            proposed_tail_cursor = Some(
                                PiJsonlV2Cursor {
                                    binding_id: binding_id.to_string(),
                                    byte_offset: offset,
                                    native_entry_anchor: Some(terminal_leaf_id.to_string()),
                                    relation: TimelineBoundaryRelation::After,
                                }
                                .encode(),
                            );
                        }
                    }
                    Err(error) => errors.push(error),
                }
            }
            Err(error) => errors.push(format!("source resolution failed: {error}")),
        }
    }

    let proposed_event_timeline_boundary = proposed_tail_cursor
        .as_ref()
        .map(|cursor| serde_json::json!({ "position": "tail", "cursor": cursor }));
    RepairCandidate {
        event_id: row.event_id,
        session_id: row.session_id,
        turn_id: row.turn_id,
        terminal_leaf_id: row.terminal_leaf_id,
        binding_id: row.binding_id,
        source_file,
        proposed_tail_cursor,
        proposed_event_timeline_boundary,
        status: if errors.is_empty() {
            "repairable"
        } else {
            "blocked"
        },
        errors,
    }
}

fn required<'a>(
    value: &'a Option<String>,
    name: &str,
    errors: &mut Vec<String>,
) -> Option<&'a str> {
    match value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Some(value),
        None => {
            errors.push(format!("missing {name}"));
            None
        }
    }
}

fn locate_entry_line_end(path: &Path, terminal_leaf_id: &str) -> Result<usize, String> {
    let file = File::open(path).map_err(|error| format!("source open failed: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut offset = 0usize;
    let mut matches = Vec::new();

    loop {
        line.clear();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("source read failed: {error}"))?;
        if bytes == 0 {
            break;
        }
        offset = offset
            .checked_add(bytes)
            .ok_or_else(|| "source offset overflow".to_string())?;
        let value: Value = serde_json::from_slice(&line).map_err(|error| {
            format!("source contains invalid JSONL before terminal entry: {error}")
        })?;
        if value.get("id").and_then(Value::as_str) == Some(terminal_leaf_id) {
            matches.push(offset);
        }
    }

    match matches.as_slice() {
        [offset] => Ok(*offset),
        [] => Err("terminal_leaf_id was not found in the resolved JSONL source".to_string()),
        _ => Err("terminal_leaf_id is ambiguous in the resolved JSONL source".to_string()),
    }
}

fn validate_offset(
    offset: usize,
    binding_id: &str,
    head_cursor: Option<&str>,
    next_head_cursor: Option<&str>,
    errors: &mut Vec<String>,
) {
    match head_cursor {
        Some(cursor) => match PiJsonlV2Cursor::decode(cursor, binding_id) {
            Ok(head) if offset > head.byte_offset => {}
            Ok(_) => errors.push("terminal entry precedes the turn head boundary".to_string()),
            Err(error) => errors.push(format!("turn head cursor is invalid: {error}")),
        },
        None => errors.push("turn has no head cursor".to_string()),
    }
    if let Some(cursor) = next_head_cursor {
        match PiJsonlV2Cursor::decode(cursor, binding_id) {
            Ok(next_head) if offset <= next_head.byte_offset => {}
            Ok(_) => errors.push("terminal entry is after the next turn head boundary".to_string()),
            Err(error) => errors.push(format!("next turn head cursor is invalid: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::locate_entry_line_end;

    #[test]
    fn locates_the_exact_terminal_entry_line_end() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("session.jsonl");
        let first = b"{\"type\":\"message\",\"id\":\"leaf\"}\n";
        let second = b"{\"type\":\"message\",\"id\":\"terminal_leaf\"}\n";
        fs::write(&source, [first.as_slice(), second.as_slice()].concat()).unwrap();

        assert_eq!(
            locate_entry_line_end(&source, "terminal_leaf").unwrap(),
            first.len() + second.len()
        );
    }

    #[test]
    fn rejects_duplicate_terminal_entry_ids() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("session.jsonl");
        fs::write(
            &source,
            concat!(
                "{\"type\":\"message\",\"id\":\"duplicate\"}\n",
                "{\"type\":\"message\",\"id\":\"duplicate\"}\n"
            ),
        )
        .unwrap();

        assert!(
            locate_entry_line_end(&source, "duplicate")
                .unwrap_err()
                .contains("ambiguous")
        );
    }
}
