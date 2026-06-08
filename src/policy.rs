use crate::{
    config::SecurityConfig,
    error::{Error, Result},
    model::{BuildRequest, TreeRegistration},
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
        match (&request.source_root, &request.source_url) {
            (Some(source_root), None) => {
                if !lexical_child_of(source_root, &self.security.source_allowlist) {
                    return Err(Error::Policy(format!(
                        "source root {source_root} is outside allowlist"
                    )));
                }
            }
            (None, Some(source_url)) => self.validate_clone_url(source_url)?,
            (Some(_), Some(_)) => {
                return Err(Error::Policy(
                    "request must not include both source_root and source_url".into(),
                ));
            }
            (None, None) => {
                return Err(Error::Policy(
                    "request must include source_root, source_url, or tree_name".into(),
                ));
            }
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

    pub fn validate_tree_registration(&self, tree: &TreeRegistration) -> Result<()> {
        validate_tree_name(&tree.name)?;
        match (&tree.source_root, &tree.source_url) {
            (Some(source_root), None) => {
                if !lexical_child_of(source_root, &self.security.source_allowlist) {
                    return Err(Error::Policy(format!(
                        "source root {source_root} is outside allowlist"
                    )));
                }
            }
            (None, Some(source_url)) => self.validate_clone_url(source_url)?,
            (Some(_), Some(_)) => {
                return Err(Error::Policy(
                    "tree must not include both source_root and source_url".into(),
                ));
            }
            (None, None) => {
                return Err(Error::Policy(
                    "tree must include source_root or source_url".into(),
                ));
            }
        }
        Ok(())
    }

    fn validate_clone_url(&self, source_url: &str) -> Result<()> {
        if source_url.trim() != source_url || source_url.is_empty() {
            return Err(Error::Policy("clone URL must be non-empty".into()));
        }
        if self
            .security
            .clone_url_allowlist
            .iter()
            .any(|prefix| source_url.starts_with(prefix))
        {
            return Ok(());
        }
        Err(Error::Policy(format!(
            "clone URL {source_url} is outside allowlist"
        )))
    }
}

fn validate_tree_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 128 {
        return Err(Error::Policy(
            "tree name must be between 1 and 128 bytes".into(),
        ));
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
    {
        return Err(Error::Policy(
            "tree name may only contain letters, digits, '.', '_', '-', and '/'".into(),
        ));
    }
    Ok(())
}
