use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use camino::{Utf8Path, Utf8PathBuf};
use sha2::{Digest, Sha256};

use crate::{
    config::Config,
    db::Store,
    error::{Error, Result},
    model::{ArtifactRecord, JobId, JobRecord},
    runtime::RuntimeCommand,
};

pub fn build_runtime_command(config: &Config, job: &JobRecord) -> Result<RuntimeCommand> {
    let job_root = config.storage.workspace_root.join(job.id.to_string());
    let source_root = job_root.join("source");
    let output_root = job_root.join("output");
    prepare_source_checkout(
        source_location(&job.request)?,
        job.request.git_ref.as_deref(),
        &source_root,
    )?;
    fs::create_dir_all(output_root.as_std_path())?;
    let script = build_script(&job.request)?;

    Ok(RuntimeCommand {
        program: "sh".into(),
        args: vec!["-c".into(), script],
        source_root,
        output_root,
        log_path: config.storage.log_root.join(format!("{}.log", job.id)),
    })
}

fn source_location(request: &crate::model::BuildRequest) -> Result<&str> {
    match (&request.source_root, &request.source_url) {
        (Some(source_root), None) => Ok(source_root.as_str()),
        (None, Some(source_url)) => Ok(source_url.as_str()),
        (Some(_), Some(_)) => Err(Error::Config(
            "request must not include both source_root and source_url".into(),
        )),
        (None, None) => Err(Error::Config("request has no resolved source".into())),
    }
}

fn prepare_source_checkout(source: &str, git_ref: Option<&str>, dest: &Utf8Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent.as_std_path())?;
    }
    if dest.as_std_path().exists() {
        fs::remove_dir_all(dest.as_std_path())?;
    }
    run_command(
        Command::new("git")
            .arg("clone")
            .arg("--no-hardlinks")
            .arg("--no-checkout")
            .arg(source)
            .arg(dest.as_str()),
    )?;
    let commit = resolve_commit(dest, git_ref.unwrap_or("HEAD"))?;
    run_command(
        Command::new("git")
            .arg("-C")
            .arg(dest.as_str())
            .arg("checkout")
            .arg("--detach")
            .arg(commit.trim()),
    )?;
    Ok(())
}

fn resolve_commit(source_root: &Utf8Path, git_ref: &str) -> Result<String> {
    let mut candidates = vec![git_ref.to_string()];
    if !git_ref.starts_with("refs/") {
        candidates.push(format!("refs/remotes/origin/{git_ref}"));
        candidates.push(format!("refs/heads/{git_ref}"));
        candidates.push(format!("refs/tags/{git_ref}"));
    }

    let mut last_error = None;
    for candidate in candidates {
        match run_command(
            Command::new("git")
                .arg("-C")
                .arg(source_root.as_str())
                .arg("rev-parse")
                .arg("--verify")
                .arg(format!("{candidate}^{{commit}}")),
        ) {
            Ok(commit) => return Ok(commit),
            Err(err) => last_error = Some(err),
        }
    }
    Err(last_error.unwrap_or_else(|| Error::Runtime("no git ref candidates".into())))
}

fn build_script(request: &crate::model::BuildRequest) -> Result<String> {
    let mut lines = vec![
        "set -eu".to_string(),
        make_line(&request.arch, &[request.config_target.as_str()]),
    ];

    if !request.config_fragments.is_empty() {
        let mut merge = vec![
            "\"$KBS_SOURCE_DIR\"/scripts/kconfig/merge_config.sh".to_string(),
            "-m".to_string(),
            "-O".to_string(),
            "\"$KBS_OUTPUT_DIR\"".to_string(),
            "\"$KBS_OUTPUT_DIR/.config\"".to_string(),
        ];
        for fragment in &request.config_fragments {
            merge.push(source_fragment_arg(
                request.source_root.as_deref(),
                fragment,
            )?);
        }
        lines.push(merge.join(" "));
        lines.push(make_line(&request.arch, &["olddefconfig"]));
    }

    let targets = request
        .make_targets
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    lines.push(make_line(&request.arch, &targets));
    Ok(lines.join("\n"))
}

fn make_line(arch: &str, targets: &[&str]) -> String {
    let mut parts = vec![
        "make".to_string(),
        "-C".to_string(),
        "\"$KBS_SOURCE_DIR\"".to_string(),
        "O=\"$KBS_OUTPUT_DIR\"".to_string(),
        format!("ARCH={}", shell_quote(arch)),
    ];
    parts.extend(targets.iter().map(|target| shell_quote(target)));
    parts.join(" ")
}

fn source_fragment_arg(source_root: Option<&Utf8Path>, fragment: &Utf8Path) -> Result<String> {
    let relative = if fragment.is_absolute() {
        let source_root = source_root.ok_or_else(|| {
            Error::Config("absolute config fragments require a local source_root".into())
        })?;
        fragment.strip_prefix(source_root).map_err(|_| {
            Error::Config(format!(
                "config fragment {fragment} is outside source root {source_root}"
            ))
        })?
    } else {
        fragment
    };

    let relative = relative.as_str();
    if relative == ".." || relative.starts_with("../") || relative.contains("/../") {
        return Err(Error::Config(format!(
            "config fragment {fragment} must stay inside source root"
        )));
    }
    Ok(format!("\"$KBS_SOURCE_DIR\"/{}", shell_quote(relative)))
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn run_command(command: &mut Command) -> Result<String> {
    let output = command.output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    Err(Error::Runtime(format!(
        "command {:?} failed: {}{}",
        command,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )))
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
