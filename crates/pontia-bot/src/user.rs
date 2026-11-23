use std::{
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use crate::identity::{BotIdentity, BotRole, IdentityError};

pub const USER_CHARACTER_LIMIT: usize = 2_000;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateUser {
    pub old_text: String,
    pub new_text: String,
}

#[derive(Debug, Serialize)]
pub struct UserFile {
    pub text: String,
    pub character_limit: usize,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UserRequest {
    pub session_id: String,
    pub input: UpdateUser,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    #[error("old_text was not found")]
    NotFound,
    #[error("old_text matches more than once")]
    Ambiguous,
    #[error("USER.md exceeds {USER_CHARACTER_LIMIT} characters")]
    TooLong,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Forbidden(#[from] IdentityError),
    #[error("{reason}\nCurrent USER.md:\n{current_text}")]
    Rejected {
        reason: Rejection,
        current_text: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

/// One shared instance in pontiad serializes updates from all Bot Sessions.
#[derive(Clone)]
pub struct UserService {
    path: Arc<Mutex<std::path::PathBuf>>,
}

impl UserService {
    pub fn new(pontia_home: &Path) -> Self {
        Self {
            path: Arc::new(Mutex::new(pontia_home.join("bot/USER.md"))),
        }
    }

    /// Reads USER.md for display; a missing file is empty with no update time.
    pub async fn read(&self) -> Result<UserFile, Error> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let path = path.lock().expect("USER.md lock poisoned");
            let text = read_current(&path)?;
            let updated_at = match std::fs::metadata(&*path) {
                Ok(metadata) => Some(
                    time::OffsetDateTime::from(metadata.modified()?)
                        .format(&time::format_description::well_known::Rfc3339)
                        .map_err(std::io::Error::other)?,
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            };
            Ok(UserFile {
                text,
                character_limit: USER_CHARACTER_LIMIT,
                updated_at,
            })
        })
        .await?
    }

    pub async fn read_for_handoff(&self) -> Result<String, Error> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let path = path.lock().expect("USER.md lock poisoned");
            read_current(&path)
        })
        .await?
    }

    async fn update(&self, input: UpdateUser) -> Result<(), Error> {
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let path = path.lock().expect("USER.md lock poisoned");
            let current_text = read_current(&path)?;
            let next = if input.old_text.is_empty() {
                format!("{current_text}{}", input.new_text)
            } else {
                let Some(index) = current_text.find(&input.old_text) else {
                    return Err(Error::Rejected {
                        reason: Rejection::NotFound,
                        current_text,
                    });
                };
                if current_text.rfind(&input.old_text) != Some(index) {
                    return Err(Error::Rejected {
                        reason: Rejection::Ambiguous,
                        current_text,
                    });
                }
                current_text.replacen(&input.old_text, &input.new_text, 1)
            };
            if next.chars().count() > USER_CHARACTER_LIMIT {
                return Err(Error::Rejected {
                    reason: Rejection::TooLong,
                    current_text,
                });
            }
            let parent = path.parent().expect("USER.md has a parent directory");
            std::fs::create_dir_all(parent)?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
            temporary.write_all(next.as_bytes())?;
            temporary.persist(&*path).map_err(|error| error.error)?;
            Ok(())
        })
        .await?
    }
}

fn read_current(path: &Path) -> Result<String, Error> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

#[derive(Clone)]
pub struct UserCommands {
    service: UserService,
    identity: Arc<dyn BotIdentity>,
}

impl UserCommands {
    pub fn new(service: UserService, identity: Arc<dyn BotIdentity>) -> Self {
        Self { service, identity }
    }

    pub async fn execute(&self, request: UserRequest) -> Result<(), Error> {
        let access = self.identity.resolve(&request.session_id).await?;
        if access.role != BotRole::Bot {
            return Err(IdentityError.into());
        }
        self.service.update(request.input).await
    }
}
