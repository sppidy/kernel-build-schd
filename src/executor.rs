use crate::{config::Config, error::Result, model::JobRecord, runtime::RuntimeCommand};

pub fn build_runtime_command(config: &Config, job: &JobRecord) -> Result<RuntimeCommand> {
    Ok(RuntimeCommand {
        program: "make".into(),
        args: job.request.make_targets.clone(),
        workspace: config.storage.workspace_root.join(job.id.to_string()),
        log_path: config.storage.log_root.join(format!("{}.log", job.id)),
    })
}
