use sqlx::SqlitePool;

mod events;
mod git_status;
mod sessions;
mod tasks;
mod turns;
mod workspaces;

#[derive(Clone)]
pub struct ExternalQueryService {
    clients: crate::clients::ClientRegistry,
    pool: SqlitePool,
}

impl ExternalQueryService {
    pub fn with_clients(mut self, clients: crate::clients::ClientRegistry) -> Self {
        self.clients = clients;
        self
    }

    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            clients: Default::default(),
        }
    }
}
