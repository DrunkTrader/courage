use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("clip path must not be empty")]
    EmptyPath,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipRecord {
    pub id: i64,
    pub path: PathBuf,
    pub duration_ms: i64,
    pub created_at: i64,
    pub thumbnail_path: Option<PathBuf>,
}

pub struct ClipStore {
    connection: Connection,
}

impl ClipStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let connection = Connection::open(path)?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        let connection = Connection::open_in_memory()?;
        let store = Self { connection };
        store.initialize()?;
        Ok(store)
    }

    fn initialize(&self) -> Result<(), StoreError> {
        self.connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE IF NOT EXISTS clips (
                 id INTEGER PRIMARY KEY,
                 path TEXT NOT NULL UNIQUE,
                 duration_ms INTEGER NOT NULL CHECK (duration_ms >= 0),
                 created_at INTEGER NOT NULL,
                 thumbnail_path TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_clips_created_at ON clips(created_at DESC);",
        )?;
        Ok(())
    }

    pub fn insert_clip(
        &self,
        path: impl AsRef<Path>,
        duration_ms: i64,
        thumbnail_path: Option<&Path>,
    ) -> Result<i64, StoreError> {
        let path = path.as_ref();
        if path.as_os_str().is_empty() {
            return Err(StoreError::EmptyPath);
        }
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        self.connection.execute(
            "INSERT INTO clips(path, duration_ms, created_at, thumbnail_path)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET duration_ms = excluded.duration_ms,
               thumbnail_path = excluded.thumbnail_path",
            params![
                path.to_string_lossy(),
                duration_ms,
                created_at,
                thumbnail_path.map(|value| value.to_string_lossy().into_owned())
            ],
        )?;
        Ok(self.connection.last_insert_rowid())
    }

    pub fn list_clips(&self) -> Result<Vec<ClipRecord>, StoreError> {
        let mut statement = self.connection.prepare(
            "SELECT id, path, duration_ms, created_at, thumbnail_path
             FROM clips ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ClipRecord {
                id: row.get(0)?,
                path: PathBuf::from(row.get::<_, String>(1)?),
                duration_ms: row.get(2)?,
                created_at: row.get(3)?,
                thumbnail_path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initializes_and_lists_clips() {
        let store = ClipStore::in_memory().expect("store");
        store
            .insert_clip("target/example.mp4", 1_250, Some(Path::new("target/example.jpg")))
            .expect("insert");
        let clips = store.list_clips().expect("list");
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].duration_ms, 1_250);
        assert_eq!(clips[0].thumbnail_path.as_deref(), Some(Path::new("target/example.jpg")));
    }
}
