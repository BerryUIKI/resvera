use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Constraint violation: {0}")]
    Constraint(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobRecord {
    pub id: String,
    pub state: String,
    pub input_path: String,
    pub output_path: Option<String>,
    pub preview_path: Option<String>,
    pub model_id: String,
    pub model_package_version: String,
    pub model_variant_id: String,
    pub target_scale: u32,
    pub engine_id: String,
    pub provider_id: Option<String>,
    pub progress_fraction: f32,
    pub progress_stage: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct AppDatabase {
    conn: Arc<Mutex<Connection>>,
}

impl AppDatabase {
    pub fn new_in_memory() -> Result<Self, DatabaseError> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DatabaseError> {
        let conn = Connection::open(path)?;
        // Enable WAL mode and standard busy timeouts
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                state TEXT NOT NULL,
                input_path TEXT NOT NULL,
                output_path TEXT,
                preview_path TEXT,
                model_id TEXT NOT NULL,
                model_package_version TEXT NOT NULL,
                model_variant_id TEXT NOT NULL,
                target_scale INTEGER NOT NULL,
                engine_id TEXT NOT NULL,
                provider_id TEXT,
                progress_fraction REAL NOT NULL DEFAULT 0.0,
                progress_stage TEXT NOT NULL DEFAULT 'preparing',
                error_code TEXT,
                error_message TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_jobs_state ON jobs(state);
            CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            ",
        )?;
        Ok(())
    }

    pub fn insert_job(&self, job: &JobRecord) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO jobs (
                id, state, input_path, output_path, preview_path,
                model_id, model_package_version, model_variant_id, target_scale,
                engine_id, provider_id, progress_fraction, progress_stage,
                error_code, error_message, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            params![
                job.id,
                job.state,
                job.input_path,
                job.output_path,
                job.preview_path,
                job.model_id,
                job.model_package_version,
                job.model_variant_id,
                job.target_scale,
                job.engine_id,
                job.provider_id,
                job.progress_fraction,
                job.progress_stage,
                job.error_code,
                job.error_message,
                job.created_at,
                job.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn insert_batch_jobs(&self, jobs: &[JobRecord]) -> Result<(), DatabaseError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO jobs (
                    id, state, input_path, output_path, preview_path,
                    model_id, model_package_version, model_variant_id, target_scale,
                    engine_id, provider_id, progress_fraction, progress_stage,
                    error_code, error_message, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            )?;
            for job in jobs {
                stmt.execute(params![
                    job.id,
                    job.state,
                    job.input_path,
                    job.output_path,
                    job.preview_path,
                    job.model_id,
                    job.model_package_version,
                    job.model_variant_id,
                    job.target_scale,
                    job.engine_id,
                    job.provider_id,
                    job.progress_fraction,
                    job.progress_stage,
                    job.error_code,
                    job.error_message,
                    job.created_at,
                    job.updated_at
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn update_job_state(&self, id: &str, state: &str) -> Result<(), DatabaseError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE jobs SET state = ?1, updated_at = ?2 WHERE id = ?3",
            params![state, now, id],
        )?;
        Ok(())
    }

    pub fn get_job(&self, id: &str) -> Result<Option<JobRecord>, DatabaseError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, state, input_path, output_path, preview_path,
                    model_id, model_package_version, model_variant_id, target_scale,
                    engine_id, provider_id, progress_fraction, progress_stage,
                    error_code, error_message, created_at, updated_at
             FROM jobs WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(JobRecord {
                id: row.get(0)?,
                state: row.get(1)?,
                input_path: row.get(2)?,
                output_path: row.get(3)?,
                preview_path: row.get(4)?,
                model_id: row.get(5)?,
                model_package_version: row.get(6)?,
                model_variant_id: row.get(7)?,
                target_scale: row.get(8)?,
                engine_id: row.get(9)?,
                provider_id: row.get(10)?,
                progress_fraction: row.get(11)?,
                progress_stage: row.get(12)?,
                error_code: row.get(13)?,
                error_message: row.get(14)?,
                created_at: row.get(15)?,
                updated_at: row.get(16)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Performs the crash recovery sweep on startup:
    /// In-flight jobs ('preparing', 'running', 'finalizing') are converted to 'interrupted'.
    /// Returns the count of recovered jobs.
    pub fn run_crash_recovery_sweep(&self) -> Result<usize, DatabaseError> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let affected = conn.execute(
            "UPDATE jobs 
             SET state = 'interrupted', updated_at = ?1
             WHERE state IN ('preparing', 'running', 'finalizing')",
            params![now],
        )?;
        Ok(affected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sample_job(id: &str, state: &str) -> JobRecord {
        let now = chrono::Utc::now().to_rfc3339();
        JobRecord {
            id: id.to_string(),
            state: state.to_string(),
            input_path: "/path/to/input.png".to_string(),
            output_path: None,
            preview_path: None,
            model_id: "realesrgan-x4plus".to_string(),
            model_package_version: "1.0.0".to_string(),
            model_variant_id: "default".to_string(),
            target_scale: 4,
            engine_id: "ort".to_string(),
            provider_id: Some("cpu".to_string()),
            progress_fraction: 0.0,
            progress_stage: "preparing".to_string(),
            error_code: None,
            error_message: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    #[test]
    fn test_batch_atomic_insert() {
        let db = AppDatabase::new_in_memory().unwrap();
        let jobs = vec![
            make_sample_job("job-1", "queued"),
            make_sample_job("job-2", "queued"),
            make_sample_job("job-3", "queued"),
        ];

        db.insert_batch_jobs(&jobs).unwrap();

        assert_eq!(db.get_job("job-1").unwrap().unwrap().state, "queued");
        assert_eq!(db.get_job("job-2").unwrap().unwrap().state, "queued");
        assert_eq!(db.get_job("job-3").unwrap().unwrap().state, "queued");
    }

    #[test]
    fn test_crash_recovery_sweep() {
        let db = AppDatabase::new_in_memory().unwrap();
        let jobs = vec![
            make_sample_job("j-queued", "queued"),
            make_sample_job("j-prep", "preparing"),
            make_sample_job("j-run", "running"),
            make_sample_job("j-fin", "finalizing"),
            make_sample_job("j-succ", "succeeded"),
            make_sample_job("j-fail", "failed"),
            make_sample_job("j-cancel", "cancelled"),
        ];

        db.insert_batch_jobs(&jobs).unwrap();

        // Simulate crash recovery sweep
        let recovered = db.run_crash_recovery_sweep().unwrap();
        assert_eq!(recovered, 3); // j-prep, j-run, j-fin were in-flight

        // Verify states
        assert_eq!(db.get_job("j-queued").unwrap().unwrap().state, "queued");
        assert_eq!(db.get_job("j-prep").unwrap().unwrap().state, "interrupted");
        assert_eq!(db.get_job("j-run").unwrap().unwrap().state, "interrupted");
        assert_eq!(db.get_job("j-fin").unwrap().unwrap().state, "interrupted");
        assert_eq!(db.get_job("j-succ").unwrap().unwrap().state, "succeeded");
        assert_eq!(db.get_job("j-fail").unwrap().unwrap().state, "failed");
        assert_eq!(db.get_job("j-cancel").unwrap().unwrap().state, "cancelled");
    }
}
