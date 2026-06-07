use std::{borrow::Cow, fmt, str::FromStr};

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(Uuid);

impl JobId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for JobId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "job_{}", self.0.simple())
    }
}

impl FromStr for JobId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let raw = value
            .strip_prefix("job_")
            .ok_or_else(|| Error::Config("job id must start with job_".into()))?;
        Uuid::parse_str(raw)
            .map(Self)
            .map_err(|err| Error::Config(format!("invalid job id: {err}")))
    }
}

impl JsonSchema for JobId {
    fn schema_name() -> Cow<'static, str> {
        "JobId".into()
    }

    fn json_schema(gen: &mut SchemaGenerator) -> Schema {
        String::json_schema(gen)
    }
}
