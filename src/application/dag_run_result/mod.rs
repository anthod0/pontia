use super::*;

mod aggregate;
mod payload;
mod runs;
mod signals;
mod submit;
mod types;

use payload::*;
use types::*;

#[derive(Clone)]
pub struct DagRunResultService {
    pool: SqlitePool,
}

impl DagRunResultService {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}
