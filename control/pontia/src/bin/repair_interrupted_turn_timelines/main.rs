use std::{process::ExitCode, str::FromStr};

use pontia_agent_clients::pi::raw_transcripts::PiAgentBindingResolver;
use sqlx::{ConnectOptions, SqlitePool, sqlite::SqliteConnectOptions};

mod application;
mod args;
mod backup_validation;
mod candidate_loading;
mod planning;
mod timeline_validation;

use args::parse_args;
use backup_validation::validate_backup;
use planning::{RepairPlan, RepairSummary, build_candidate};

type RepairError = Box<dyn std::error::Error>;

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

async fn run() -> Result<usize, RepairError> {
    let args = parse_args()?;
    if args.apply {
        validate_backup(&args.database, args.backup.as_deref())?;
    }
    let options = SqliteConnectOptions::from_str(args.database.to_string_lossy().as_ref())?
        .read_only(!args.apply)
        .create_if_missing(false)
        .disable_statement_logging();
    let pool = SqlitePool::connect_with(options).await?;
    let rows = candidate_loading::load_candidates(&pool).await?;
    let resolver = PiAgentBindingResolver::new();
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
        application::apply_candidates(&pool, &candidates).await?
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
