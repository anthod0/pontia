use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionModel {
    pub id: String,
    pub name: String,
    pub description: String,
}
