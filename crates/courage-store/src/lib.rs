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
    pub favorite: bool,
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
                 thumbnail_path TEXT,
                 favorite INTEGER NOT NULL DEFAULT 0
              );
             CREATE INDEX IF NOT EXISTS idx_clips_created_at ON clips(created_at DESC);",
        )?;
        let has_favorite = self.connection.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('clips') WHERE name = 'favorite'",
            [],
            |row| row.get::<_, i64>(0),
        )? > 0;
        if !has_favorite {
            self.connection
                .execute("ALTER TABLE clips ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0", [])?;
        }
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
            "SELECT id, path, duration_ms, created_at, thumbnail_path, favorite
             FROM clips ORDER BY created_at DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ClipRecord {
                id: row.get(0)?,
                path: PathBuf::from(row.get::<_, String>(1)?),
                duration_ms: row.get(2)?,
                created_at: row.get(3)?,
                thumbnail_path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
                favorite: row.get::<_, i64>(5)? != 0,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(StoreError::from)
    }

    pub fn contains_path(&self, path: impl AsRef<Path>) -> Result<bool, StoreError> {
        let exists = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM clips WHERE path = ?1)",
            [path.as_ref().to_string_lossy().as_ref()],
            |row| row.get(0),
        )?;
        Ok(exists)
    }

    pub fn set_favorite(&self, id: i64, favorite: bool) -> Result<bool, StoreError> {
        Ok(self
            .connection
            .execute("UPDATE clips SET favorite = ?1 WHERE id = ?2", (favorite as i64, id))?
            > 0)
    }

    pub fn delete_clip(&self, id: i64) -> Result<bool, StoreError> {
        Ok(self.connection.execute("DELETE FROM clips WHERE id = ?1", [id])? > 0)
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
        assert!(!clips[0].favorite);
        assert!(store.contains_path("target/example.mp4").expect("contains"));
        store.set_favorite(clips[0].id, true).expect("favorite");
        assert!(store.list_clips().unwrap()[0].favorite);
        assert!(store.delete_clip(clips[0].id).expect("delete"));
        assert!(!store.contains_path("target/example.mp4").expect("not contains"));
    }

    #[test]
    fn migrates_existing_clip_table_without_favorite_column() {
        let path = std::env::temp_dir().join(format!(
            "courage-store-migration-{}-{}.sqlite",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        let connection = Connection::open(&path).expect("create legacy database");
        connection
            .execute_batch(
                "CREATE TABLE clips (
                    id INTEGER PRIMARY KEY,
                    path TEXT NOT NULL UNIQUE,
                    duration_ms INTEGER NOT NULL,
                    created_at INTEGER NOT NULL,
                    thumbnail_path TEXT
                );",
            )
            .expect("legacy schema");
        drop(connection);

        let store = ClipStore::open(&path).expect("migrate");
        store.insert_clip("legacy.mp4", 1000, None).expect("insert migrated row");
        let clip = store.list_clips().unwrap().pop().unwrap();
        assert!(!clip.favorite);
        store.set_favorite(clip.id, true).expect("set migrated favorite");
        assert!(store.list_clips().unwrap()[0].favorite);
        drop(store);
        std::fs::remove_file(path).expect("remove test database");
    }
}
