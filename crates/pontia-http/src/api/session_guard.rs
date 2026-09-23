use pontia_application::ExternalQueryService;

use super::response::ApiError;

pub(super) async fn ensure_session_exists(
    service: &ExternalQueryService,
    session_id: &str,
) -> Result<(), ApiError> {
    service
        .get_session(session_id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("session {session_id} not found")))?;
    Ok(())
}
