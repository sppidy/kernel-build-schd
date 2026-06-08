use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use time::OffsetDateTime;

use crate::{
    error::{Error, Result},
    model::{
        ArtifactRecord, BuildRequest, JobId, JobRecord, JobState, TreeRecord, TreeRegistration,
    },
};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                request_json TEXT NOT NULL,
                state TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                failure_json TEXT
            );

            CREATE TABLE IF NOT EXISTS job_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id TEXT NOT NULL,
                event TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS artifacts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                job_id TEXT NOT NULL,
                path TEXT NOT NULL,
                kind TEXT NOT NULL,
                bytes INTEGER NOT NULL,
                sha256 TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS trees (
                name TEXT PRIMARY KEY,
                source_root TEXT,
                source_url TEXT,
                default_ref TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                CHECK (
                    (source_root IS NOT NULL AND source_url IS NULL) OR
                    (source_root IS NULL AND source_url IS NOT NULL)
                )
            );
            "#,
        )?;
        Ok(())
    }

    pub fn enqueue(&self, request: BuildRequest) -> Result<JobRecord> {
        let id = JobId::new();
        let now = OffsetDateTime::now_utc();
        let record = JobRecord {
            id,
            request,
            state: JobState::Queued,
            created_at: now,
            updated_at: now,
            failure: None,
        };
        self.conn.execute(
            "INSERT INTO jobs (id, request_json, state, created_at, updated_at, failure_json)
             VALUES (?1, ?2, ?3, ?4, ?5, NULL)",
            params![
                record.id.to_string(),
                serde_json::to_string(&record.request)?,
                state_name(record.state),
                record.created_at.unix_timestamp_nanos().to_string(),
                record.updated_at.unix_timestamp_nanos().to_string(),
            ],
        )?;
        self.record_event(id, "queued")?;
        Ok(record)
    }

    pub fn get_job(&self, id: JobId) -> Result<JobRecord> {
        let row = self
            .conn
            .query_row(
                "SELECT id, request_json, state, created_at, updated_at, failure_json
                 FROM jobs WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .map_err(Error::from)?;

        let (id_text, request_json, state_text, created_at, updated_at, failure_json) = row;
        Ok(JobRecord {
            id: id_text.parse()?,
            request: serde_json::from_str(&request_json)?,
            state: parse_state(&state_text)?,
            created_at: parse_timestamp(&created_at)?,
            updated_at: parse_timestamp(&updated_at)?,
            failure: match failure_json {
                Some(value) => Some(serde_json::from_str(&value)?),
                None => None,
            },
        })
    }

    pub fn set_state(&self, id: JobId, next: JobState) -> Result<()> {
        let current = self.get_job(id)?;
        current.state.transition_to(next)?;
        self.conn.execute(
            "UPDATE jobs SET state = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                state_name(next),
                OffsetDateTime::now_utc().unix_timestamp_nanos().to_string(),
                id.to_string()
            ],
        )?;
        self.record_event(id, state_name(next))?;
        Ok(())
    }

    pub fn events(&self, id: JobId) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT event FROM job_events WHERE job_id = ?1 ORDER BY id ASC")?;
        let rows = stmt.query_map(params![id.to_string()], |row| row.get::<_, String>(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }

    pub fn next_queued(&self) -> Result<Option<JobRecord>> {
        let id_text = {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM jobs WHERE state = 'queued' ORDER BY updated_at ASC LIMIT 1",
            )?;
            let mut rows = stmt.query([])?;
            if let Some(row) = rows.next()? {
                Some(row.get::<_, String>(0)?)
            } else {
                None
            }
        };

        match id_text {
            Some(value) => Ok(Some(self.get_job(value.parse()?)?)),
            None => Ok(None),
        }
    }

    pub fn insert_artifact(&self, artifact: &ArtifactRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO artifacts (job_id, path, kind, bytes, sha256)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                artifact.job_id.to_string(),
                artifact.path.as_str(),
                artifact.kind.as_str(),
                artifact.bytes as i64,
                artifact.sha256.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn list_artifacts(&self, job_id: JobId) -> Result<Vec<ArtifactRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, kind, bytes, sha256 FROM artifacts WHERE job_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![job_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut artifacts = Vec::new();
        for row in rows {
            let (path, kind, bytes, sha256) = row?;
            artifacts.push(ArtifactRecord {
                job_id,
                path: path.into(),
                kind,
                bytes: bytes as u64,
                sha256,
            });
        }
        Ok(artifacts)
    }

    pub fn count_by_state(&self, state: JobState) -> Result<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE state = ?1",
            params![state_name(state)],
            |row| row.get(0),
        )?;
        Ok(count as u64)
    }

    pub fn list_jobs(&self) -> Result<Vec<JobRecord>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM jobs ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut jobs = Vec::new();
        for row in rows {
            jobs.push(self.get_job(row?.parse()?)?);
        }
        Ok(jobs)
    }

    pub fn register_tree(&self, tree: TreeRegistration) -> Result<TreeRecord> {
        let now = OffsetDateTime::now_utc();
        let existing_created_at = self
            .conn
            .query_row(
                "SELECT created_at FROM trees WHERE name = ?1",
                params![tree.name.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        self.conn.execute(
            "INSERT INTO trees (name, source_root, source_url, default_ref, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(name) DO UPDATE SET
                source_root = excluded.source_root,
                source_url = excluded.source_url,
                default_ref = excluded.default_ref,
                updated_at = excluded.updated_at",
            params![
                tree.name.as_str(),
                tree.source_root.as_ref().map(|value| value.as_str()),
                tree.source_url.as_deref(),
                tree.default_ref.as_deref(),
                existing_created_at.unwrap_or_else(|| now.unix_timestamp_nanos().to_string()),
                now.unix_timestamp_nanos().to_string(),
            ],
        )?;
        self.get_tree(&tree.name)
    }

    pub fn get_tree(&self, name: &str) -> Result<TreeRecord> {
        let row = self
            .conn
            .query_row(
                "SELECT name, source_root, source_url, default_ref, created_at, updated_at
                 FROM trees WHERE name = ?1",
                params![name],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )
            .map_err(Error::from)?;
        tree_from_row(row)
    }

    pub fn list_trees(&self) -> Result<Vec<TreeRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, source_root, source_url, default_ref, created_at, updated_at
             FROM trees ORDER BY name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let mut trees = Vec::new();
        for row in rows {
            trees.push(tree_from_row(row?)?);
        }
        Ok(trees)
    }

    pub fn remove_tree(&self, name: &str) -> Result<bool> {
        let changed = self
            .conn
            .execute("DELETE FROM trees WHERE name = ?1", params![name])?;
        Ok(changed > 0)
    }

    fn record_event(&self, id: JobId, event: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO job_events (job_id, event, created_at) VALUES (?1, ?2, ?3)",
            params![
                id.to_string(),
                event,
                OffsetDateTime::now_utc().unix_timestamp_nanos().to_string()
            ],
        )?;
        Ok(())
    }
}

fn tree_from_row(
    row: (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
    ),
) -> Result<TreeRecord> {
    let (name, source_root, source_url, default_ref, created_at, updated_at) = row;
    Ok(TreeRecord {
        name,
        source_root: source_root.map(Into::into),
        source_url,
        default_ref,
        created_at: parse_timestamp(&created_at)?,
        updated_at: parse_timestamp(&updated_at)?,
    })
}

pub(crate) fn state_name(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Preparing => "preparing",
        JobState::Running => "running",
        JobState::Collecting => "collecting",
        JobState::Succeeded => "succeeded",
        JobState::Canceling => "canceling",
        JobState::Canceled => "canceled",
        JobState::Failed => "failed",
        JobState::TimedOut => "timed_out",
    }
}

fn parse_state(value: &str) -> Result<JobState> {
    match value {
        "queued" => Ok(JobState::Queued),
        "preparing" => Ok(JobState::Preparing),
        "running" => Ok(JobState::Running),
        "collecting" => Ok(JobState::Collecting),
        "succeeded" => Ok(JobState::Succeeded),
        "canceling" => Ok(JobState::Canceling),
        "canceled" => Ok(JobState::Canceled),
        "failed" => Ok(JobState::Failed),
        "timed_out" => Ok(JobState::TimedOut),
        _ => Err(Error::Config(format!("unknown job state {value}"))),
    }
}

fn parse_timestamp(value: &str) -> Result<OffsetDateTime> {
    let nanos = value
        .parse::<i128>()
        .map_err(|err| Error::Config(format!("invalid timestamp: {err}")))?;
    OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .map_err(|err| Error::Config(format!("invalid timestamp: {err}")))
}
