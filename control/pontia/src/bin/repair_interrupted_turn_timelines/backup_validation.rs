use std::path::Path;

use super::RepairError;

pub(super) fn validate_backup(database: &Path, backup: Option<&Path>) -> Result<(), RepairError> {
    let backup = backup.ok_or("--apply requires --backup")?;
    if database.canonicalize()? == backup.canonicalize()? {
        return Err("backup path must differ from the database path".into());
    }
    if backup.metadata()?.len() == 0 {
        return Err("backup file is empty".into());
    }
    Ok(())
}
