use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub actor: String,
    pub action: String,
    pub target: String,
    pub created_at: OffsetDateTime,
}

impl AuditEvent {
    pub fn new(
        actor: impl Into<String>,
        action: impl Into<String>,
        target: impl Into<String>,
    ) -> Self {
        Self {
            actor: actor.into(),
            action: action.into(),
            target: target.into(),
            created_at: OffsetDateTime::now_utc(),
        }
    }
}
