use crate::{
    config::SecurityConfig,
    error::{Error, Result},
    model::BuildRequest,
    paths::lexical_child_of,
};

#[derive(Debug, Clone)]
pub struct Policy {
    security: SecurityConfig,
}

impl Policy {
    pub fn new(security: SecurityConfig) -> Self {
        Self { security }
    }

    pub fn validate_request(&self, request: &BuildRequest) -> Result<()> {
        if !lexical_child_of(&request.source_root, &self.security.source_allowlist) {
            return Err(Error::Policy(format!(
                "source root {} is outside allowlist",
                request.source_root
            )));
        }

        for item in &request.env {
            if self
                .security
                .denied_env
                .iter()
                .any(|denied| denied == &item.key)
            {
                return Err(Error::Policy(format!(
                    "denied environment variable {}",
                    item.key
                )));
            }
        }

        if request.make_targets.is_empty() {
            return Err(Error::Policy("at least one make target is required".into()));
        }

        Ok(())
    }
}
