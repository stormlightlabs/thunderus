//! Schema migration logic for the memory store
//!
//! Tracks applied migrations and applies pending ones up to the current schema version.

use crate::error::{Error, Result};
use crate::schema::{BM25_COLUMN_WEIGHTS, SCHEMA_SQL, SCHEMA_VERSION};
use rusqlite::Connection;
use tracing::{debug, info, trace};

/// BM25 column weights parsed from the constant for programmatic access
///
/// Order: title, headings, tags, body, path, kind
pub const BM25_WEIGHTS: [f64; 6] = [10.0, 5.0, 3.0, 1.0, 1.0, 1.0];

/// Manages schema migrations for the memory store
pub struct MigrationManager;

impl MigrationManager {
    /// Get the current schema version from the database
    ///
    /// Returns 0 if the schema_version table doesn't exist or is empty.
    pub fn get_current_version(conn: &Connection) -> Result<i32> {
        let table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_version')",
                [],
                |row| row.get(0),
            )
            .map_err(|e| Error::database(format!("Failed to check schema_version table: {e}")))?;

        if !table_exists {
            trace!("schema_version table does not exist, returning version 0");
            return Ok(0);
        }

        let version: Option<i32> = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |row| row.get(0))
            .map_err(|e| Error::database(format!("Failed to query schema version: {e}")))?;

        Ok(version.unwrap_or(0))
    }

    /// Apply pending migrations up to SCHEMA_VERSION
    ///
    /// This is idempotent - running it multiple times is safe.
    pub fn migrate(conn: &Connection) -> Result<()> {
        let current_version = Self::get_current_version(conn)?;
        debug!(
            "Current schema version: {}, target: {}",
            current_version, SCHEMA_VERSION
        );

        if current_version >= SCHEMA_VERSION {
            trace!("Schema is up to date, no migration needed");
            return Ok(());
        }

        info!(
            "Migrating schema from version {} to {}",
            current_version, SCHEMA_VERSION
        );

        if current_version == 0 {
            Self::apply_v1_migration(conn)?;
        }

        info!("Schema migration complete");
        Ok(())
    }

    fn apply_v1_migration(conn: &Connection) -> Result<()> {
        debug!("Applying v1 migration");

        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| Error::database(format!("Failed to apply v1 schema: {e}")))?;

        trace!("v1 migration applied successfully");
        Ok(())
    }
}

fn _assert_bm25_weights_match() {
    let parsed: Vec<f64> = BM25_COLUMN_WEIGHTS
        .split(',')
        .map(|s| s.trim().parse::<f64>().unwrap())
        .collect();
    assert_eq!(parsed.len(), BM25_WEIGHTS.len());
    for (i, (p, w)) in parsed.iter().zip(BM25_WEIGHTS.iter()).enumerate() {
        assert!(
            (p - w).abs() < f64::EPSILON,
            "BM25_WEIGHTS[{}] mismatch: {} vs {}",
            i,
            p,
            w
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_get_current_version_fresh_db() {
        let conn = Connection::open_in_memory().unwrap();
        let version = MigrationManager::get_current_version(&conn).unwrap();
        assert_eq!(version, 0);
    }

    #[test]
    fn test_migrate_applies_schema_and_sets_version() {
        let conn = Connection::open_in_memory().unwrap();
        MigrationManager::migrate(&conn).unwrap();

        let version = MigrationManager::get_current_version(&conn).unwrap();
        assert_eq!(version, SCHEMA_VERSION);

        let table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='memory_docs')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(table_exists);
    }

    #[test]
    fn test_migrate_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();

        MigrationManager::migrate(&conn).unwrap();
        let version1 = MigrationManager::get_current_version(&conn).unwrap();

        MigrationManager::migrate(&conn).unwrap();
        let version2 = MigrationManager::get_current_version(&conn).unwrap();

        assert_eq!(version1, version2);
        assert_eq!(version2, SCHEMA_VERSION);
    }

    #[test]
    fn test_bm25_weights_match_constant() {
        _assert_bm25_weights_match();
    }

    #[test]
    fn test_bm25_weights_array() {
        assert_eq!(BM25_WEIGHTS.len(), 6);
        assert_eq!(BM25_WEIGHTS[0], 10.0); // title
        assert_eq!(BM25_WEIGHTS[1], 5.0); // headings
        assert_eq!(BM25_WEIGHTS[2], 3.0); // tags
        assert_eq!(BM25_WEIGHTS[3], 1.0); // body
        assert_eq!(BM25_WEIGHTS[4], 1.0); // path
        assert_eq!(BM25_WEIGHTS[5], 1.0); // kind
    }
}
