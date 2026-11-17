use std::sync::Arc;

use pontia_agent_control::{
    SessionMessagesPage, SessionMessagesQuery, TurnTimelineService, TurnTimelineServiceError,
};
use serde::Deserialize;

use crate::identity::{BotIdentity, BotRole, IdentityError};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionMessagesRequest {
    pub session_id: String,
    pub input: SessionMessagesQuery,
}

#[derive(Debug)]
pub enum Error {
    Forbidden,
    Timeline(TurnTimelineServiceError),
}

impl From<IdentityError> for Error {
    fn from(_: IdentityError) -> Self {
        Self::Forbidden
    }
}

#[derive(Clone)]
pub struct SessionMessages {
    timeline: TurnTimelineService,
    identity: Arc<dyn BotIdentity>,
}

impl SessionMessages {
    pub fn new(timeline: TurnTimelineService, identity: Arc<dyn BotIdentity>) -> Self {
        Self { timeline, identity }
    }

    pub async fn execute(
        &self,
        request: SessionMessagesRequest,
    ) -> Result<SessionMessagesPage, Error> {
        let access = self.identity.resolve(&request.session_id).await?;
        if !matches!(
            access.role,
            BotRole::Bot | BotRole::Retrieval | BotRole::Dreaming
        ) {
            return Err(Error::Forbidden);
        }
        self.timeline
            .messages(request.input)
            .await
            .map_err(Error::Timeline)
    }
}
