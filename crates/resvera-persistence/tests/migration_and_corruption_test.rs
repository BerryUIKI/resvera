use resvera_persistence::{AppDatabase, DatabaseError, CURRENT_SCHEMA_VERSION};
use rusqlite::Connection;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn test_corrupt_database_detection() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("corrupt.sqlite3");

    // Write completely invalid/corrupted bytes
    {
        let mut file = File::create(&db_path).unwrap();
        file.write_all(b"NOT_A_VALID_SQLITE_DATABASE_HEADER_DATA_1234567890")
            .unwrap();
        file.sync_all().unwrap();
    }

    let result = AppDatabase::open(&db_path);
    assert!(result.is_err(), "Opening corrupt database should fail");
    match result.unwrap_err() {
        DatabaseError::CorruptDatabase(msg) => {
            assert!(!msg.is_empty());
        }
        DatabaseError::Sqlite(e) => {
            // Some rusqlite versions might return SQLite error directly before or during pragma
            assert!(!e.to_string().is_empty());
        }
        other => panic!("Expected CorruptDatabase or Sqlite error, got: {:?}", other),
    }
}

#[test]
fn test_incompatible_schema_version_detection() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("future_version.sqlite3");

    // Create a database with a future user_version (e.g. 999)
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.pragma_update(None, "user_version", 999).unwrap();
    }

    let result = AppDatabase::open(&db_path);
    assert!(
        result.is_err(),
        "Opening future schema database should fail"
    );
    match result.unwrap_err() {
        DatabaseError::IncompatibleSchemaVersion { found, supported } => {
            assert_eq!(found, 999);
            assert_eq!(supported, CURRENT_SCHEMA_VERSION);
        }
        other => panic!("Expected IncompatibleSchemaVersion error, got: {:?}", other),
    }
}

#[test]
fn test_legacy_database_migration_sets_user_version() {
    let temp = tempdir().unwrap();
    let db_path = temp.path().join("legacy.sqlite3");

    // Create a legacy v0 database
    {
        let legacy = Connection::open(&db_path).unwrap();
        legacy
            .execute_batch(
                "CREATE TABLE jobs (
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
                );",
            )
            .unwrap();
        let v: u32 = legacy
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(v, 0);
    }

    // Opening with AppDatabase should migrate and set user_version
    let db = AppDatabase::open(&db_path).expect("Should successfully open and migrate legacy db");
    drop(db);

    // Verify user_version is now CURRENT_SCHEMA_VERSION
    let conn = Connection::open(&db_path).unwrap();
    let v: u32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(v, CURRENT_SCHEMA_VERSION);
}
