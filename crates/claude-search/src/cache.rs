use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

/// Simple key-value cache backed by SQLite.
/// Used for caching project summaries and other computed data.
pub struct Cache {
    conn: Connection,
}

impl Cache {
    pub fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;

        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;

            CREATE TABLE IF NOT EXISTS cache (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER
            );
            ",
        )?;

        Ok(Self { conn })
    }

    /// Open cache in the `.claude-tools` directory of the given project root.
    pub fn open_for_project(project_root: &Path) -> Result<Self> {
        let db_path = project_root.join(".claude-tools").join("cache.db");
        Self::open(&db_path)
    }

    /// Get a cached value by key. Returns None if not found or expired.
    pub fn get(&self, key: &str) -> Result<Option<String>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let result = self.conn.query_row(
            "SELECT value FROM cache WHERE key = ?1 AND (expires_at IS NULL OR expires_at > ?2)",
            params![key, now],
            |row| row.get::<_, String>(0),
        );

        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Set a cached value with optional TTL in seconds.
    pub fn set(&self, key: &str, value: &str, ttl_secs: Option<u64>) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let expires_at = ttl_secs.map(|ttl| now + ttl as i64);

        self.conn.execute(
            "INSERT INTO cache (key, value, created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET value = ?2, created_at = ?3, expires_at = ?4",
            params![key, value, now, expires_at],
        )?;

        Ok(())
    }

    /// Remove a cached value.
    pub fn remove(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM cache WHERE key = ?1", params![key])?;
        Ok(())
    }

    /// Remove all expired entries.
    pub fn cleanup(&self) -> Result<usize> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let count = self.conn.execute(
            "DELETE FROM cache WHERE expires_at IS NOT NULL AND expires_at <= ?1",
            params![now],
        )?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_cache_get_set() {
        let dir = TempDir::new().unwrap();
        let cache = Cache::open(&dir.path().join("cache.db")).unwrap();

        cache.set("key1", "value1", None).unwrap();
        assert_eq!(cache.get("key1").unwrap(), Some("value1".to_string()));
        assert_eq!(cache.get("nonexistent").unwrap(), None);
    }

    #[test]
    fn test_cache_remove() {
        let dir = TempDir::new().unwrap();
        let cache = Cache::open(&dir.path().join("cache.db")).unwrap();

        cache.set("key1", "value1", None).unwrap();
        cache.remove("key1").unwrap();
        assert_eq!(cache.get("key1").unwrap(), None);
    }
}
