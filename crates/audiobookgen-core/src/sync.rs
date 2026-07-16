use crate::model::ProgressState;
use anyhow::Result;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize)]
struct DeviceManifest<'a> {
    format: &'static str,
    version: u32,
    book_id: String,
    profile_id: String,
    narrated_epub: &'a str,
    progress: Option<&'a ProgressState>,
}

pub fn write_folder_package(folder: &Path, book_id: uuid::Uuid, profile_id: uuid::Uuid, narrated_epub: &Path, progress: Option<&ProgressState>) -> Result<PathBuf> {
    let destination = folder.join("AudiobookGen").join(book_id.to_string());
    std::fs::create_dir_all(&destination)?;
    let epub_name = narrated_epub.file_name().and_then(|value| value.to_str()).unwrap_or("book.epub");
    std::fs::copy(narrated_epub, destination.join(epub_name))?;
    let manifest = DeviceManifest { format: "audiobookgen-device-package", version: 1, book_id: book_id.to_string(), profile_id: profile_id.to_string(), narrated_epub: epub_name, progress };
    std::fs::write(destination.join("manifest.json"), serde_json::to_vec_pretty(&manifest)?)?;
    Ok(destination)
}
