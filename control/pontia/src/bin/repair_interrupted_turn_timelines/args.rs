use std::{env, path::PathBuf};

#[derive(Debug)]
pub(super) struct Args {
    pub(super) database: PathBuf,
    pub(super) apply: bool,
    pub(super) backup: Option<PathBuf>,
}

pub(super) fn parse_args() -> Result<Args, String> {
    let mut database = None;
    let mut apply = false;
    let mut backup = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--database" => database = args.next().map(PathBuf::from),
            "--apply" => apply = true,
            "--backup" => backup = args.next().map(PathBuf::from),
            "--help" | "-h" => {
                return Err(
                    "usage: repair_interrupted_turn_timelines --database <pontia.db> [--apply --backup <backup.db>]"
                        .to_string(),
                );
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    let database = database.ok_or_else(|| "--database is required".to_string())?;
    if !apply && backup.is_some() {
        return Err("--backup requires --apply".to_string());
    }
    Ok(Args {
        database,
        apply,
        backup,
    })
}
