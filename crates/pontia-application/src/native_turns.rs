use crate::ReportedFact;
use pontia_core::{Error, Result, ids::new_turn_id};
use sqlx::SqlitePool;
fn string<'a>(value: &'a serde_json::Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| Error::Domain(format!("Missing {field}")))
}
pub(crate) async fn native_turn_identity(pool: &SqlitePool, fact: &ReportedFact) -> Result<String> {
    let native = string(&fact.data, "native_turn_id")?;
    let runtime = string(&fact.data, "runtime_instance_id")?;
    let current: Option<String> =
        sqlx::query_scalar("SELECT runtime_instance_id FROM runtime_bindings WHERE session_id=?")
            .bind(&fact.session_id)
            .fetch_optional(pool)
            .await?
            .flatten();
    if current.as_deref() != Some(runtime) {
        return Err(Error::StateConflict(
            "Client fact belongs to an obsolete runtime".into(),
        ));
    }
    sqlx::query("INSERT INTO native_turn_bindings(session_id,client_turn_id,turn_id) SELECT ?,?,? WHERE EXISTS (SELECT 1 FROM runtime_bindings WHERE session_id=? AND runtime_instance_id=?) ON CONFLICT(session_id,client_turn_id) DO NOTHING")
        .bind(&fact.session_id).bind(native).bind(new_turn_id().to_string()).bind(&fact.session_id).bind(runtime).execute(pool).await?;
    sqlx::query_scalar(
        "SELECT turn_id FROM native_turn_bindings WHERE session_id=? AND client_turn_id=?",
    )
    .bind(&fact.session_id)
    .bind(native)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        Error::StateConflict("Runtime changed before reserving native Turn identity".into())
    })
}
