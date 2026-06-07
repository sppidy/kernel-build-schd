use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use camino::Utf8PathBuf;
use sha2::{Digest, Sha256};

use crate::{
    config::Config,
    db::Store,
    error::{Error, Result},
    model::{ArtifactRecord, JobId, JobRecord},
    runtime::RuntimeCommand,
};

pub fn build_runtime_command(config: &Config, job: &JobRecord) -> Result<RuntimeCommand> {
    Ok(RuntimeCommand {
        program: "make".into(),
        args: job.request.make_targets.clone(),
        workspace: config.storage.workspace_root.join(job.id.to_string()),
        log_path: config.storage.log_root.join(format!("{}.log", job.id)),
    })
}

pub fn write_combined_log(path: impl AsRef<Path>, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    Ok(())
}

pub fn collect_artifacts(
    config: &Config,
    store: &Store,
    job_id: JobId,
    output_dir: impl AsRef<Path>,
) -> Result<Vec<ArtifactRecord>> {
    fs::create_dir_all(config.storage.artifact_root.as_std_path())?;
    let job = store.get_job(job_id)?;
    let mut records = Vec::new();

    for pattern in &job.request.artifact_patterns {
        let source = output_dir.as_ref().join(pattern);
        if !source.is_file() {
            continue;
        }

        let file_name = source
            .file_name()
            .ok_or_else(|| Error::Config("artifact path has no file name".into()))?;
        let dest = PathBuf::from(config.storage.artifact_root.as_str())
            .join(job_id.to_string())
            .join(file_name);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&source, &dest)?;
        let bytes = fs::read(&dest)?;
        let hash = Sha256::digest(&bytes);
        let record = ArtifactRecord {
            job_id,
            path: Utf8PathBuf::from_path_buf(dest)
                .map_err(|_| Error::Config("artifact path is not utf-8".into()))?,
            kind: "kernel-output".into(),
            bytes: bytes.len() as u64,
            sha256: format!("{hash:x}"),
        };
        store.insert_artifact(&record)?;
        records.push(record);
    }

    Ok(records)
}
