use pontia_core::{Error, Result};
use serde_json::Value;
use sqlx::{
    Connection,
    sqlite::{SqliteConnectOptions, SqliteConnection},
};
use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
};

pub async fn current_path(home: &Path, thread: &str, stored: Option<&Path>) -> Result<PathBuf> {
    let mut databases = std::fs::read_dir(home)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .filter_map(|entry| {
            let name = entry.file_name();
            let version = name
                .to_str()?
                .strip_prefix("state_")?
                .strip_suffix(".sqlite")?
                .parse::<u32>()
                .ok()?;
            Some((version, entry.path()))
        })
        .collect::<Vec<_>>();
    databases.sort_by_key(|(version, _)| *version);
    if let Some((_, path)) = databases.last() {
        let mut db = SqliteConnection::connect_with(
            &SqliteConnectOptions::new().filename(path).read_only(true),
        )
        .await?;
        let path: Option<String> =
            sqlx::query_scalar("SELECT rollout_path FROM threads WHERE id=?")
                .bind(thread)
                .fetch_optional(&mut db)
                .await?;
        if let Some(path) = path {
            return existing(Path::new(&path));
        }
    }
    existing(stored.ok_or_else(|| unavailable("Codex rollout path is missing"))?)
}

fn unavailable(message: impl std::fmt::Display) -> Error {
    Error::CapabilityUnavailable(format!("source_unavailable: {message}"))
}

fn existing(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.into());
    }
    let compressed = PathBuf::from(format!("{}.zst", path.display()));
    if compressed.is_file() {
        return Ok(compressed);
    }
    Err(unavailable(format!(
        "Codex rollout {} is missing",
        path.display()
    )))
}

fn reader(path: &Path) -> Result<Box<dyn BufRead>> {
    let file = File::open(path).map_err(unavailable)?;
    let input: Box<dyn Read> = if path.extension().is_some_and(|ext| ext == "zst") {
        Box::new(zstd::stream::read::Decoder::new(file)?)
    } else {
        Box::new(file)
    };
    Ok(Box::new(BufReader::new(input)))
}

pub fn header(path: &Path) -> Result<Value> {
    let mut line = String::new();
    reader(path)?.read_line(&mut line)?;
    let value: Value = serde_json::from_str(&line)?;
    if value["type"] != "session_meta" {
        return Err(Error::Domain("Invalid Codex rollout header".into()));
    }
    Ok(value)
}

fn find_rollout(directory: &Path, id: &str) -> Result<Option<PathBuf>> {
    if !directory.exists() {
        return Ok(None);
    }
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(path) = find_rollout(&entry.path(), id)? {
                return Ok(Some(path));
            }
        } else {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(&format!("-{id}.jsonl"))
                || name.ends_with(&format!("-{id}.jsonl.zst"))
                || name.ends_with(&format!("_{id}.jsonl"))
                || name.ends_with(&format!("_{id}.jsonl.zst"))
            {
                return Ok(Some(entry.path()));
            }
        }
    }
    Ok(None)
}

pub fn records(path: &Path) -> Result<Vec<(u64, Value)>> {
    let home = path
        .ancestors()
        .find_map(|ancestor| {
            matches!(
                ancestor.file_name().and_then(|n| n.to_str()),
                Some("sessions" | "archived_sessions")
            )
            .then(|| ancestor.parent())
            .flatten()
        })
        .unwrap_or_else(|| path.parent().expect("rollout parent"));
    let mut path = path.to_path_buf();
    let mut end: Option<u64> = None;
    let mut seen = HashSet::new();
    let mut segments = Vec::new();
    loop {
        if !seen.insert(path.clone()) {
            return Err(Error::Domain("Cyclic Codex history_base".into()));
        }
        let meta = header(&path)?;
        let base = &meta["payload"]["history_base"];
        let mut segment = Vec::new();
        let mut input = reader(&path)?;
        let mut offset = 0;
        let mut line = String::new();
        loop {
            line.clear();
            if input.read_line(&mut line)? == 0 {
                break;
            }
            if !line.ends_with('\n') {
                break;
            }
            let next = offset + line.len() as u64;
            if end.is_some_and(|end| next > end) {
                break;
            }
            segment.push(serde_json::from_str::<Value>(&line)?);
            offset = next;
        }
        if end.is_some_and(|end| offset != end) {
            return Err(Error::Domain("Invalid Codex history_base cutoff".into()));
        }
        segments.push(segment);
        if base.is_null() {
            break;
        }
        let id = base["thread_id"]
            .as_str()
            .ok_or_else(|| Error::Domain("Codex history_base has no rollout ID".into()))?;
        let ordinal = base["end_ordinal_exclusive"]
            .as_u64()
            .ok_or_else(|| Error::Domain("Codex history_base has no ordinal".into()))?;
        let offset = base["end_byte_offset"]
            .as_u64()
            .ok_or_else(|| Error::Domain("Codex history_base has no offset".into()))?;
        if ordinal == 0 {
            break;
        }
        let referenced =
            if home.join("sessions").is_dir() || home.join("archived_sessions").is_dir() {
                match find_rollout(&home.join("sessions"), id)? {
                    Some(path) => Some(path),
                    None => find_rollout(&home.join("archived_sessions"), id)?,
                }
            } else {
                find_rollout(home, id)?
            };
        path = referenced
            .ok_or_else(|| unavailable(format!("Referenced Codex rollout {id} is missing")))?;
        end = Some(offset);
    }
    Ok(segments
        .into_iter()
        .rev()
        .flatten()
        .enumerate()
        .map(|(index, record)| (index as u64, record))
        .collect())
}
