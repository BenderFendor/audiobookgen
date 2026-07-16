use crate::platform::SleepGuard;
use anyhow::{Context, Result, anyhow, bail};
use audiobookgen_core::Core;
use audiobookgen_core::cache::segment_cache_key;
use audiobookgen_core::epub::{extract_cover, inspect_epub, parse_selected_chapters};
use audiobookgen_core::export::{
    ExportChapter, ExportFragment, ExportManifest, export_m4a_chapters, export_m4b,
    export_narrated_epub,
};
use audiobookgen_core::model::*;
use audiobookgen_core::narration::{PLANNER_VERSION, plan_fragments};
use audiobookgen_core::normalize::{NORMALIZATION_VERSION, normalize_for_speech};
use audiobookgen_core::sync::write_folder_package;
use audiobookgen_core::worker::{WorkerRequest, WorkerSupervisor};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppRuntime {
    pub core: Arc<Core>,
    worker: Arc<Mutex<Option<Arc<WorkerSupervisor>>>>,
    jobs: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    worker_root: PathBuf,
    python: PathBuf,
    bootstrap_python: Option<PathBuf>,
    ffmpeg: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct ModelStatus {
    pub installed: bool,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GenerationProgress {
    job_id: String,
    book_id: String,
    completed: usize,
    total: usize,
    fragment_id: Option<String>,
    state: &'static str,
    message: Option<String>,
}

impl AppRuntime {
    pub fn new(data_dir: PathBuf, resource_dir: PathBuf) -> Result<Self> {
        let core = Arc::new(Core::open(&data_dir)?);
        let development_worker =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../services/tts-worker");
        let bundled_worker = resource_dir.join("tts-worker");
        let worker_root = std::env::var_os("AUDIOBOOKGEN_WORKER_ROOT")
            .map(PathBuf::from)
            .or_else(|| bundled_worker.exists().then_some(bundled_worker))
            .unwrap_or(development_worker);
        let explicit_python = std::env::var_os("AUDIOBOOKGEN_PYTHON").map(PathBuf::from);
        let managed_python = managed_python(&data_dir);
        let (python, bootstrap_python) = match explicit_python {
            Some(value) => (value, None),
            None => (
                managed_python,
                Some(PathBuf::from(if cfg!(windows) {
                    "python.exe"
                } else {
                    "python3"
                })),
            ),
        };
        let ffmpeg = std::env::var_os("AUDIOBOOKGEN_FFMPEG")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(if cfg!(windows) {
                    "ffmpeg.exe"
                } else {
                    "ffmpeg"
                })
            });
        Ok(Self {
            core,
            worker: Arc::new(Mutex::new(None)),
            jobs: Arc::new(Mutex::new(HashMap::new())),
            worker_root,
            python,
            bootstrap_python,
            ffmpeg,
        })
    }

    async fn worker(&self) -> Result<Arc<WorkerSupervisor>> {
        let mut slot = self.worker.lock().await;
        self.ensure_worker_environment().await?;
        if let Some(worker) = slot.as_ref() {
            return Ok(worker.clone());
        }
        let args = vec!["-m".to_owned(), "audiobookgen_worker.main".to_owned()];
        let worker = Arc::new(WorkerSupervisor::spawn(&self.python, &args, &self.worker_root).await
            .with_context(|| format!("Kokoro worker could not start. Install the worker environment or set AUDIOBOOKGEN_PYTHON. Python: {}", self.python.display()))?);
        worker
            .ping()
            .await
            .context("Kokoro worker did not answer its startup check")?;
        *slot = Some(worker.clone());
        Ok(worker)
    }

    async fn ensure_worker_environment(&self) -> Result<()> {
        if self.bootstrap_python.is_none() || self.python.exists() {
            return Ok(());
        }
        let bootstrap = self.bootstrap_python.clone().expect("checked above");
        let python = self.python.clone();
        let worker_root = self.worker_root.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let venv_dir = python
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| anyhow!("invalid managed Python path"))?;
            std::fs::create_dir_all(venv_dir.parent().unwrap_or(venv_dir))?;
            let uv = std::env::var_os("AUDIOBOOKGEN_UV")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "uv.exe" } else { "uv" }));
            let uv_available = std::process::Command::new(&uv)
                .arg("--version")
                .output()
                .map(|output| output.status.success())
                .unwrap_or(false);
            if uv_available {
                // uv downloads a managed interpreter, so this works even when the
                // system Python is newer than Kokoro supports (>=3.10,<3.14).
                let status = std::process::Command::new(&uv)
                    .args(["venv", "--python", WORKER_PYTHON_VERSION])
                    .arg(venv_dir)
                    .status()
                    .context("creating the managed Python environment with uv")?;
                if !status.success() {
                    bail!("uv venv failed with {status}");
                }
                let status = std::process::Command::new(&uv)
                    .args(["pip", "install", "--python"])
                    .arg(&python)
                    .arg(&worker_root)
                    .status()
                    .context("installing the Kokoro worker dependencies with uv")?;
                if !status.success() {
                    bail!("uv pip install failed with {status}");
                }
                return Ok(());
            }
            let status = std::process::Command::new(&bootstrap)
                .args(["-m", "venv"])
                .arg(venv_dir)
                .status()
                .with_context(|| {
                    format!(
                        "Python 3.10-3.13 (or uv) is required to install Kokoro. Could not run {}",
                        bootstrap.display()
                    )
                })?;
            if !status.success() {
                bail!("creating the managed Python environment failed with {status}");
            }
            let status = std::process::Command::new(&python)
                .args([
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "--no-input",
                ])
                .arg(&worker_root)
                .status()
                .context("installing the Kokoro worker dependencies")?;
            if !status.success() {
                bail!("installing the Kokoro worker failed with {status}");
            }
            Ok(())
        })
        .await??;
        Ok(())
    }

    fn model_dir(&self) -> PathBuf {
        self.core.data_dir.join("models/kokoro-82m")
    }
    fn cache_dir(&self) -> PathBuf {
        self.core.data_dir.join("cache/segments")
    }
}

// Kokoro's dependency chain supports >=3.10,<3.14; keep this inside that range.
const WORKER_PYTHON_VERSION: &str = "3.12";

fn managed_python(data_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        data_dir.join("worker-venv/Scripts/python.exe")
    } else {
        data_dir.join("worker-venv/bin/python")
    }
}

const SUPPORTED_VOICES: &[&str] = &[
    "af_heart",
    "af_bella",
    "af_nicole",
    "am_adam",
    "am_michael",
    "bf_emma",
    "bm_george",
];

fn command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
fn parse_uuid(value: &str, label: &str) -> Result<Uuid> {
    Uuid::parse_str(value).with_context(|| format!("invalid {label}"))
}

#[tauri::command]
pub async fn inspect_epub_file(path: String) -> std::result::Result<ImportReview, String> {
    tokio::task::spawn_blocking(move || inspect_epub(path))
        .await
        .map_err(command_error)?
        .map_err(command_error)
}

#[tauri::command]
pub async fn import_epub(
    runtime: tauri::State<'_, AppRuntime>,
    review: ImportReview,
    selection: ImportSelection,
) -> std::result::Result<BookDetail, String> {
    let runtime = runtime.inner().clone();
    tokio::task::spawn_blocking(move || import_epub_impl(&runtime, review, selection))
        .await
        .map_err(command_error)?
        .map_err(command_error)
}

fn import_epub_impl(
    runtime: &AppRuntime,
    review: ImportReview,
    selection: ImportSelection,
) -> Result<BookDetail> {
    if selection.selected_chapter_indices.is_empty() {
        bail!("select at least one chapter to narrate");
    }
    let fresh = inspect_epub(&review.source_path)?;
    if fresh.source_sha256 != review.source_sha256 {
        bail!("the EPUB changed after import review; review it again");
    }
    if fresh.drm_detected {
        bail!("this EPUB contains encrypted resources; AudiobookGen does not remove DRM");
    }
    if let Some(existing) = runtime
        .core
        .db
        .find_book_by_source_sha(&fresh.source_sha256)?
    {
        return Ok(existing);
    }

    let book_id = Uuid::new_v4();
    let book_dir = runtime
        .core
        .data_dir
        .join("books")
        .join(book_id.to_string());
    std::fs::create_dir_all(&book_dir)?;
    let source_path = book_dir.join("source.epub");
    std::fs::copy(&fresh.source_path, &source_path)
        .context("copying EPUB into the local library")?;
    let cover_path = fresh.cover_entry.as_ref().and_then(|entry| {
        let extension = Path::new(entry)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("img");
        let destination = book_dir.join(format!("cover.{extension}"));
        extract_cover(&source_path, entry, &destination)
            .ok()
            .map(|_| destination)
    });
    let now = Utc::now();
    let profile = NarrationProfile {
        id: Uuid::new_v4(),
        book_id,
        name: "Default narrator".into(),
        voice: "af_heart".into(),
        speed: 1.0,
        model_revision: "hexgrad/Kokoro-82M".into(),
        model_sha256: None,
        normalization_version: NORMALIZATION_VERSION.into(),
        planner_version: PLANNER_VERSION.into(),
        created_at: now,
    };
    let parsed = parse_selected_chapters(&source_path, &fresh, &selection, book_id)?;
    let mut chapters = Vec::new();
    let mut fragments = Vec::new();
    for parsed_chapter in parsed {
        let mut chapter = parsed_chapter.chapter;
        let mut chapter_fragments = plan_fragments(book_id, parsed_chapter.blocks, &profile, &[]);
        for (index, fragment) in chapter_fragments.iter_mut().enumerate() {
            fragment.index = index;
        }
        chapter.fragment_count = chapter_fragments.len();
        chapters.push(chapter);
        fragments.extend(chapter_fragments);
    }
    if fragments.is_empty() {
        bail!("the selected chapters contain no narratable text");
    }
    let summary = BookSummary {
        id: book_id,
        title: fresh.title,
        authors: fresh.authors,
        language: fresh.language,
        cover_path,
        source_path,
        layout: fresh.layout,
        chapter_count: chapters.len(),
        generated_sentences: 0,
        total_sentences: fragments.len(),
        active_profile_id: Some(profile.id),
        created_at: now,
        updated_at: now,
    };
    runtime.core.db.insert_book(
        &summary,
        &fresh.source_sha256,
        &chapters,
        &profile,
        &fragments,
    )?;
    runtime
        .core
        .db
        .get_book(book_id)?
        .ok_or_else(|| anyhow!("imported book was not persisted"))
}

#[tauri::command]
pub async fn list_books(
    runtime: tauri::State<'_, AppRuntime>,
) -> std::result::Result<Vec<BookSummary>, String> {
    runtime.core.db.list_books().map_err(command_error)
}

#[tauri::command]
pub async fn get_book(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
) -> std::result::Result<Option<BookDetail>, String> {
    runtime
        .core
        .db
        .get_book(parse_uuid(&book_id, "book id").map_err(command_error)?)
        .map_err(command_error)
}

#[tauri::command]
pub async fn get_chapter_fragments(
    runtime: tauri::State<'_, AppRuntime>,
    chapter_id: String,
) -> std::result::Result<Vec<Fragment>, String> {
    runtime
        .core
        .db
        .fragments_for_chapter(parse_uuid(&chapter_id, "chapter id").map_err(command_error)?)
        .map_err(command_error)
}

#[tauri::command]
pub async fn create_narration_profile(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
    input: CreateNarrationProfile,
) -> std::result::Result<NarrationProfile, String> {
    let book_id = parse_uuid(&book_id, "book id").map_err(command_error)?;
    if input.name.trim().is_empty() {
        return Err("profile name cannot be empty".into());
    }
    if !SUPPORTED_VOICES.contains(&input.voice.as_str()) {
        return Err("unsupported Kokoro voice".into());
    }
    if !(0.5..=2.0).contains(&input.speed) {
        return Err("speed must be between 0.5 and 2.0".into());
    }
    let profile = NarrationProfile {
        id: Uuid::new_v4(),
        book_id,
        name: input.name.trim().into(),
        voice: input.voice,
        speed: input.speed,
        model_revision: "hexgrad/Kokoro-82M".into(),
        model_sha256: None,
        normalization_version: NORMALIZATION_VERSION.into(),
        planner_version: PLANNER_VERSION.into(),
        created_at: Utc::now(),
    };
    runtime
        .core
        .db
        .insert_profile(&profile)
        .map_err(command_error)?;
    Ok(profile)
}

#[tauri::command]
pub async fn set_active_profile(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
    profile_id: String,
) -> std::result::Result<(), String> {
    runtime
        .core
        .db
        .set_active_profile(
            parse_uuid(&book_id, "book id").map_err(command_error)?,
            parse_uuid(&profile_id, "profile id").map_err(command_error)?,
        )
        .map_err(command_error)
}

#[tauri::command]
pub async fn save_pronunciation_rule(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
    pattern: String,
    replacement: String,
    case_sensitive: bool,
) -> std::result::Result<PronunciationRule, String> {
    if pattern.trim().is_empty() || replacement.trim().is_empty() {
        return Err("pronunciation text cannot be empty".into());
    }
    let rule = PronunciationRule {
        id: Uuid::new_v4(),
        book_id: Some(parse_uuid(&book_id, "book id").map_err(command_error)?),
        pattern,
        replacement,
        case_sensitive,
        created_at: Utc::now(),
    };
    runtime
        .core
        .db
        .save_pronunciation_rule(&rule)
        .map_err(command_error)?;
    Ok(rule)
}

#[tauri::command]
pub async fn save_progress(
    runtime: tauri::State<'_, AppRuntime>,
    progress: ProgressState,
) -> std::result::Result<(), String> {
    runtime
        .core
        .db
        .save_progress(&progress)
        .map_err(command_error)
}

#[tauri::command]
pub async fn load_progress(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
) -> std::result::Result<Option<ProgressState>, String> {
    runtime
        .core
        .db
        .load_progress(parse_uuid(&book_id, "book id").map_err(command_error)?)
        .map_err(command_error)
}

#[tauri::command]
pub async fn model_status(
    runtime: tauri::State<'_, AppRuntime>,
) -> std::result::Result<ModelStatus, String> {
    let path = runtime.model_dir();
    Ok(ModelStatus {
        installed: (path.join("config.json").exists() && path.join("kokoro-v1_0.pth").exists())
            || path.join("MOCK_MODEL").exists(),
        path,
    })
}

#[tauri::command]
pub async fn download_model(
    runtime: tauri::State<'_, AppRuntime>,
) -> std::result::Result<Value, String> {
    let worker = runtime.worker().await.map_err(command_error)?;
    let response = worker
        .request(WorkerRequest::DownloadModel {
            id: Uuid::new_v4().to_string(),
            model_dir: runtime.model_dir(),
        })
        .await
        .map_err(command_error)?;
    Ok(response.payload)
}

#[tauri::command]
pub async fn preview_voice(
    runtime: tauri::State<'_, AppRuntime>,
    text: String,
    voice: String,
    speed: f32,
) -> std::result::Result<String, String> {
    if !SUPPORTED_VOICES.contains(&voice.as_str()) {
        return Err("unsupported Kokoro voice".into());
    }
    ensure_model(runtime.inner()).map_err(command_error)?;
    let output = runtime
        .core
        .data_dir
        .join("cache")
        .join(format!("preview-{}.wav", Uuid::new_v4()));
    let worker = runtime.worker().await.map_err(command_error)?;
    worker
        .request(WorkerRequest::Generate {
            id: Uuid::new_v4().to_string(),
            text,
            voice,
            speed,
            output_path: output.clone(),
            model_dir: runtime.model_dir(),
        })
        .await
        .map_err(command_error)?;
    Ok(output.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn queue_generation(
    app: AppHandle,
    runtime: tauri::State<'_, AppRuntime>,
    request: QueueGeneration,
) -> std::result::Result<String, String> {
    ensure_model(runtime.inner()).map_err(command_error)?;
    let runtime = runtime.inner().clone();
    let job_id = Uuid::new_v4().to_string();
    let cancelled = Arc::new(AtomicBool::new(false));
    runtime
        .jobs
        .lock()
        .await
        .insert(job_id.clone(), cancelled.clone());
    let task_job_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_generation(&app, &runtime, &task_job_id, request, cancelled).await;
        if let Err(error) = result {
            let _ = app.emit(
                "generation-progress",
                GenerationProgress {
                    job_id: task_job_id.clone(),
                    book_id: String::new(),
                    completed: 0,
                    total: 0,
                    fragment_id: None,
                    state: "failed",
                    message: Some(error.to_string()),
                },
            );
        }
        runtime.jobs.lock().await.remove(&task_job_id);
    });
    Ok(job_id)
}

async fn run_generation(
    app: &AppHandle,
    runtime: &AppRuntime,
    job_id: &str,
    request: QueueGeneration,
    cancelled: Arc<AtomicBool>,
) -> Result<()> {
    let _sleep_guard = SleepGuard::acquire();
    let detail = runtime
        .core
        .db
        .get_book(request.book_id)?
        .ok_or_else(|| anyhow!("book not found"))?;
    let profile = runtime
        .core
        .db
        .get_profile(request.book_id, request.profile_id)?
        .ok_or_else(|| anyhow!("narration profile not found"))?;
    let rules = runtime.core.db.pronunciation_rules(request.book_id)?;
    let selected_indices: HashSet<usize> = match request.mode {
        GenerationMode::FullBook => detail
            .chapters
            .iter()
            .filter(|chapter| chapter.selected)
            .map(|chapter| chapter.index)
            .collect(),
        GenerationMode::CurrentAndNext => {
            let current = request.current_chapter_index.unwrap_or_else(|| {
                detail
                    .chapters
                    .first()
                    .map(|chapter| chapter.index)
                    .unwrap_or(0)
            });
            [current, current.saturating_add(1)].into_iter().collect()
        }
        GenerationMode::SelectedChapters => request.selected_chapter_indices.into_iter().collect(),
    };
    let fragments: Vec<Fragment> = runtime
        .core
        .db
        .fragments_for_book(request.book_id)?
        .into_iter()
        .filter(|fragment| selected_indices.contains(&fragment.chapter_index))
        .collect();
    if fragments.is_empty() {
        bail!("no narratable sentences matched this generation request");
    }
    let total = fragments.len();
    app.emit(
        "generation-progress",
        GenerationProgress {
            job_id: job_id.into(),
            book_id: request.book_id.to_string(),
            completed: 0,
            total,
            fragment_id: None,
            state: "running",
            message: None,
        },
    )?;
    let worker = runtime.worker().await?;
    let mut completed = 0usize;
    for mut fragment in fragments {
        if cancelled.load(Ordering::Relaxed) {
            app.emit(
                "generation-progress",
                GenerationProgress {
                    job_id: job_id.into(),
                    book_id: request.book_id.to_string(),
                    completed,
                    total,
                    fragment_id: None,
                    state: "cancelled",
                    message: None,
                },
            )?;
            return Ok(());
        }
        fragment.spoken_text = normalize_for_speech(&fragment.source_text, &rules);
        fragment.cache_key = segment_cache_key(&fragment, &profile);
        if let Some(existing) = runtime.core.db.generated_segment(fragment.id, profile.id)? {
            if existing.cache_key == fragment.cache_key && existing.audio_path.exists() {
                completed += 1;
                app.emit(
                    "generation-progress",
                    GenerationProgress {
                        job_id: job_id.into(),
                        book_id: request.book_id.to_string(),
                        completed,
                        total,
                        fragment_id: Some(fragment.id.to_string()),
                        state: "generating",
                        message: Some("Reused cached sentence".into()),
                    },
                )?;
                continue;
            }
        }
        app.emit(
            "generation-progress",
            GenerationProgress {
                job_id: job_id.into(),
                book_id: request.book_id.to_string(),
                completed,
                total,
                fragment_id: Some(fragment.id.to_string()),
                state: "generating",
                message: None,
            },
        )?;
        let final_path = runtime
            .cache_dir()
            .join(format!("{}.wav", fragment.cache_key));
        let temporary = runtime.cache_dir().join(format!(
            ".{}.{}.part.wav",
            fragment.cache_key,
            Uuid::new_v4()
        ));
        let (duration_ms, sample_rate) = if fragment.kind == FragmentKind::SceneBreak {
            write_silence(&temporary, 40)?;
            (40, 24_000)
        } else {
            let response = worker
                .request(WorkerRequest::Generate {
                    id: Uuid::new_v4().to_string(),
                    text: fragment.spoken_text.clone(),
                    voice: profile.voice.clone(),
                    speed: profile.speed,
                    output_path: temporary.clone(),
                    model_dir: runtime.model_dir(),
                })
                .await?;
            (
                response
                    .duration_ms
                    .ok_or_else(|| anyhow!("worker did not report audio duration"))?,
                response.sample_rate.unwrap_or(24_000),
            )
        };
        if final_path.exists() {
            let _ = std::fs::remove_file(&temporary);
        } else {
            std::fs::rename(&temporary, &final_path)
                .context("committing generated audio to cache")?;
        }
        runtime.core.db.save_generated_segment(&GeneratedSegment {
            fragment_id: fragment.id,
            profile_id: profile.id,
            cache_key: fragment.cache_key,
            audio_path: final_path,
            duration_ms,
            sample_rate,
            created_at: Utc::now(),
        })?;
        completed += 1;
        app.emit(
            "generation-progress",
            GenerationProgress {
                job_id: job_id.into(),
                book_id: request.book_id.to_string(),
                completed,
                total,
                fragment_id: Some(fragment.id.to_string()),
                state: "generating",
                message: None,
            },
        )?;
    }
    app.emit(
        "generation-progress",
        GenerationProgress {
            job_id: job_id.into(),
            book_id: request.book_id.to_string(),
            completed,
            total,
            fragment_id: None,
            state: "complete",
            message: None,
        },
    )?;
    Ok(())
}

fn write_silence(path: &Path, duration_ms: u64) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 24_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for _ in 0..(duration_ms * 24) {
        writer.write_sample(0i16)?;
    }
    writer.finalize()?;
    Ok(())
}

#[tauri::command]
pub async fn cancel_generation(
    runtime: tauri::State<'_, AppRuntime>,
    job_id: String,
) -> std::result::Result<(), String> {
    if let Some(flag) = runtime.jobs.lock().await.get(&job_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
pub async fn get_generated_audio(
    runtime: tauri::State<'_, AppRuntime>,
    fragment_id: String,
    profile_id: String,
) -> std::result::Result<Option<String>, String> {
    Ok(runtime
        .core
        .db
        .generated_segment(
            parse_uuid(&fragment_id, "fragment id").map_err(command_error)?,
            parse_uuid(&profile_id, "profile id").map_err(command_error)?,
        )
        .map_err(command_error)?
        .filter(|segment| segment.audio_path.exists())
        .map(|segment| segment.audio_path.to_string_lossy().into_owned()))
}

fn ensure_model(runtime: &AppRuntime) -> Result<()> {
    if (runtime.model_dir().join("config.json").exists()
        && runtime.model_dir().join("kokoro-v1_0.pth").exists())
        || runtime.model_dir().join("MOCK_MODEL").exists()
    {
        Ok(())
    } else {
        bail!("Kokoro is not installed yet. Download the model from the library screen first.")
    }
}

fn export_manifest(
    runtime: &AppRuntime,
    book_id: Uuid,
    profile_id: Uuid,
) -> Result<ExportManifest> {
    let book = runtime
        .core
        .db
        .get_book(book_id)?
        .ok_or_else(|| anyhow!("book not found"))?;
    let profile = runtime
        .core
        .db
        .get_profile(book_id, profile_id)?
        .ok_or_else(|| anyhow!("profile not found"))?;
    let fragments = runtime.core.db.fragments_for_book(book_id)?;
    let mut grouped: HashMap<usize, Vec<ExportFragment>> = HashMap::new();
    let mut missing = 0usize;
    for fragment in fragments {
        match runtime.core.db.generated_segment(fragment.id, profile_id)? {
            Some(segment) if segment.audio_path.exists() => grouped
                .entry(fragment.chapter_index)
                .or_default()
                .push(ExportFragment { fragment, segment }),
            _ => missing += 1,
        }
    }
    if missing > 0 {
        bail!("{missing} sentences have not been generated for this narrator profile");
    }
    let chapters = book
        .chapters
        .iter()
        .filter_map(|chapter| {
            grouped
                .remove(&chapter.index)
                .map(|fragments| ExportChapter {
                    index: chapter.index,
                    title: chapter.title.clone(),
                    href: chapter.href.clone(),
                    fragments,
                })
        })
        .collect();
    Ok(ExportManifest {
        book,
        profile,
        chapters,
    })
}

#[tauri::command]
pub async fn export_m4a(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
    profile_id: String,
    output_dir: String,
) -> std::result::Result<Vec<String>, String> {
    let manifest = export_manifest(
        runtime.inner(),
        parse_uuid(&book_id, "book id").map_err(command_error)?,
        parse_uuid(&profile_id, "profile id").map_err(command_error)?,
    )
    .map_err(command_error)?;
    let ffmpeg = runtime.ffmpeg.clone();
    tokio::task::spawn_blocking(move || {
        export_m4a_chapters(&manifest, &ffmpeg, Path::new(&output_dir))
    })
    .await
    .map_err(command_error)?
    .map(|paths| {
        paths
            .into_iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect()
    })
    .map_err(command_error)
}

#[tauri::command]
pub async fn export_m4b_file(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
    profile_id: String,
    output_path: String,
) -> std::result::Result<String, String> {
    let manifest = export_manifest(
        runtime.inner(),
        parse_uuid(&book_id, "book id").map_err(command_error)?,
        parse_uuid(&profile_id, "profile id").map_err(command_error)?,
    )
    .map_err(command_error)?;
    let ffmpeg = runtime.ffmpeg.clone();
    let destination = PathBuf::from(output_path);
    let returned = destination.clone();
    tokio::task::spawn_blocking(move || export_m4b(&manifest, &ffmpeg, &destination))
        .await
        .map_err(command_error)?
        .map_err(command_error)?;
    Ok(returned.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn export_narrated_epub_file(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
    profile_id: String,
    output_path: String,
) -> std::result::Result<String, String> {
    let manifest = export_manifest(
        runtime.inner(),
        parse_uuid(&book_id, "book id").map_err(command_error)?,
        parse_uuid(&profile_id, "profile id").map_err(command_error)?,
    )
    .map_err(command_error)?;
    let ffmpeg = runtime.ffmpeg.clone();
    let source = manifest.book.summary.source_path.clone();
    let destination = PathBuf::from(output_path);
    let returned = destination.clone();
    tokio::task::spawn_blocking(move || {
        export_narrated_epub(&source, &manifest, &ffmpeg, &destination)
    })
    .await
    .map_err(command_error)?
    .map_err(command_error)?;
    Ok(returned.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn sync_to_folder(
    runtime: tauri::State<'_, AppRuntime>,
    book_id: String,
    profile_id: String,
    folder: String,
) -> std::result::Result<String, String> {
    let book_id = parse_uuid(&book_id, "book id").map_err(command_error)?;
    let profile_id = parse_uuid(&profile_id, "profile id").map_err(command_error)?;
    let manifest = export_manifest(runtime.inner(), book_id, profile_id).map_err(command_error)?;
    let ffmpeg = runtime.ffmpeg.clone();
    let source = manifest.book.summary.source_path.clone();
    let output = runtime
        .core
        .data_dir
        .join("exports")
        .join(format!("{book_id}-{profile_id}.epub"));
    std::fs::create_dir_all(output.parent().expect("export directory")).map_err(command_error)?;
    let output_for_export = output.clone();
    tokio::task::spawn_blocking(move || {
        export_narrated_epub(&source, &manifest, &ffmpeg, &output_for_export)
    })
    .await
    .map_err(command_error)?
    .map_err(command_error)?;
    let progress = runtime
        .core
        .db
        .load_progress(book_id)
        .map_err(command_error)?;
    let destination = write_folder_package(
        Path::new(&folder),
        book_id,
        profile_id,
        &output,
        progress.as_ref(),
    )
    .map_err(command_error)?;
    Ok(destination.to_string_lossy().into_owned())
}
