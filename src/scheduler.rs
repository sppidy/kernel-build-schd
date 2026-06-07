use std::sync::Arc;

use crate::{
    config::Config,
    db::Store,
    error::Result,
    executor::build_runtime_command,
    model::{JobId, JobRecord, JobState},
    runtime::BuildRuntime,
};

pub struct Scheduler {
    config: Config,
    store: Store,
    runtime: Arc<dyn BuildRuntime>,
}

impl Scheduler {
    pub fn new(config: Config, store: Store, runtime: Arc<dyn BuildRuntime>) -> Self {
        Self {
            config,
            store,
            runtime,
        }
    }

    pub fn get_job(&self, id: JobId) -> Result<JobRecord> {
        self.store.get_job(id)
    }

    pub async fn run_one_queued_job(&self) -> Result<Option<JobId>> {
        let Some(job) = self.store.next_queued()? else {
            return Ok(None);
        };

        self.store.set_state(job.id, JobState::Preparing)?;
        let command = build_runtime_command(&self.config, &job)?;
        self.store.set_state(job.id, JobState::Running)?;
        let exit = self.runtime.run(job.id, command).await?;
        self.store.set_state(job.id, JobState::Collecting)?;

        if exit.canceled {
            self.store.set_state(job.id, JobState::Canceled)?;
        } else if exit.code == Some(0) {
            self.store.set_state(job.id, JobState::Succeeded)?;
        } else {
            self.store.set_state(job.id, JobState::Failed)?;
        }

        Ok(Some(job.id))
    }
}
