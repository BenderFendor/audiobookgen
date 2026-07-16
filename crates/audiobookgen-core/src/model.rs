use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EpubLayout {
    Reflowable,
    Fixed,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSummary {
    pub id: Uuid,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub cover_path: Option<PathBuf>,
    pub source_path: PathBuf,
    pub layout: EpubLayout,
    pub chapter_count: usize,
    pub generated_sentences: usize,
    pub total_sentences: usize,
    pub active_profile_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterReview {
    pub index: usize,
    pub title: String,
    pub href: String,
    pub media_type: String,
    pub layout: EpubLayout,
    pub selected: bool,
    pub estimated_words: usize,
    pub footnote_count: usize,
    pub caption_count: usize,
    pub table_count: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportReview {
    pub source_path: PathBuf,
    pub source_sha256: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub identifier: Option<String>,
    pub layout: EpubLayout,
    pub drm_detected: bool,
    pub chapters: Vec<ChapterReview>,
    pub cover_entry: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportSelection {
    pub selected_chapter_indices: Vec<usize>,
    pub footnote_mode: FootnoteMode,
    pub table_mode: TableMode,
    pub caption_mode: CaptionMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FootnoteMode {
    Skip,
    Inline,
    EndOfChapter,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TableMode {
    Skip,
    Summary,
    Cells,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptionMode {
    Skip,
    Read,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookDetail {
    pub summary: BookSummary,
    pub chapters: Vec<Chapter>,
    pub profiles: Vec<NarrationProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: Uuid,
    pub book_id: Uuid,
    pub index: usize,
    pub title: String,
    pub href: String,
    pub media_type: String,
    pub layout: EpubLayout,
    pub selected: bool,
    pub fragment_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
    pub id: Uuid,
    pub book_id: Uuid,
    pub chapter_id: Uuid,
    pub chapter_index: usize,
    pub index: usize,
    pub source_text: String,
    pub spoken_text: String,
    pub kind: FragmentKind,
    pub locator: FragmentLocator,
    pub pause_after_ms: u32,
    pub cache_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FragmentKind {
    Heading,
    Sentence,
    Dialogue,
    Caption,
    Table,
    Footnote,
    SceneBreak,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentLocator {
    pub href: String,
    pub css_selector: Option<String>,
    pub text_occurrence: usize,
    pub source_text_hash: String,
    pub cfi: Option<String>,
}

pub fn default_engine() -> String {
    "kokoro".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NarrationProfile {
    pub id: Uuid,
    pub book_id: Uuid,
    pub name: String,
    #[serde(default = "default_engine")]
    pub engine: String,
    pub voice: String,
    pub speed: f32,
    pub model_revision: String,
    pub model_sha256: Option<String>,
    pub normalization_version: String,
    pub planner_version: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNarrationProfile {
    pub name: String,
    #[serde(default = "default_engine")]
    pub engine: String,
    pub voice: String,
    pub speed: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationMode {
    CurrentAndNext,
    FullBook,
    SelectedChapters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueGeneration {
    pub book_id: Uuid,
    pub profile_id: Uuid,
    pub mode: GenerationMode,
    pub current_chapter_index: Option<usize>,
    pub selected_chapter_indices: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WordTiming {
    pub word: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedSegment {
    pub fragment_id: Uuid,
    pub profile_id: Uuid,
    pub cache_key: String,
    pub audio_path: PathBuf,
    pub duration_ms: u64,
    pub sample_rate: u32,
    #[serde(default)]
    pub word_timings: Vec<WordTiming>,
    pub created_at: DateTime<Utc>,
}

/// User-tunable narration settings persisted in the library database.
/// `models_root` decides where every engine stores weights and caches.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub models_root: Option<PathBuf>,
    pub maya1_quant: String,
    pub maya1_device: String,
    pub maya1_temperature: f32,
    pub voxtral_server_url: String,
    pub voxtral_default_voice: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            models_root: None,
            maya1_quant: "Q8_0".into(),
            maya1_device: "auto".into(),
            maya1_temperature: 0.4,
            voxtral_server_url: "http://127.0.0.1:8570".into(),
            voxtral_default_voice: "narrator_female".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressState {
    pub book_id: Uuid,
    pub reading_locator: Option<FragmentLocator>,
    pub listening_fragment_id: Option<Uuid>,
    pub listening_offset_ms: u64,
    pub linked: bool,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PronunciationRule {
    pub id: Uuid,
    pub book_id: Option<Uuid>,
    pub pattern: String,
    pub replacement: String,
    pub case_sensitive: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NarrationBlock {
    pub chapter_id: Uuid,
    pub chapter_index: usize,
    pub href: String,
    pub text: String,
    pub kind: FragmentKind,
    pub occurrence: usize,
}
