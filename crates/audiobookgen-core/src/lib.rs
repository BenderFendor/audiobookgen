pub mod cache;
pub mod db;
pub mod epub;
pub mod export;
pub mod model;
pub mod narration;
pub mod normalize;
pub mod sync;
pub mod worker;

use anyhow::Result;
use db::LibraryDatabase;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Core {
    pub data_dir: PathBuf,
    pub db: LibraryDatabase,
}
impl Core {
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self> {
        let data_dir = data_dir.as_ref().to_path_buf();
        std::fs::create_dir_all(data_dir.join("books"))?;
        std::fs::create_dir_all(data_dir.join("cache/segments"))?;
        std::fs::create_dir_all(data_dir.join("models"))?;
        let db = LibraryDatabase::open(data_dir.join("library.sqlite3"))?;
        Ok(Self { data_dir, db })
    }
}
