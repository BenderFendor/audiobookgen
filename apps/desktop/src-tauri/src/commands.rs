use crate::platform::SleepGuard;
use crate::voxtral;
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
use std::io::Read;
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

pub const ENGINES: &[&str] = &["kokoro", "maya1", "voxtral"];

#[derive(Debug, Serialize)]
pub struct EngineModelStatus {
    pub engine: String,
    pub installed: bool,
    pub path: PathBuf,
    /// For maya1: which quantization the status refers to.
    pub quant: Option<String>,
    /// For voxtral: whether the worker environment contains INT4 dependencies.
    pub runtime_installed: Option<bool>,
    /// Voice embeddings found in the installed model directory.
    pub voices: Vec<String>,
    /// Human-readable accelerator/runtime compatibility detail.
    pub accelerator_message: Option<String>,
    pub hardware_supported: Option<bool>,
}

const SETTINGS_KEY: &str = "app_settings";

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelProgress {
    engine: String,
    phase: String,
    message: String,
    current: Option<u64>,
    total: Option<u64>,
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

    pub fn settings(&self) -> AppSettings {
        self.core
            .db
            .get_setting(SETTINGS_KEY)
            .ok()
            .flatten()
            .and_then(|value| serde_json::from_str(&value).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &AppSettings) -> Result<()> {
        self.core
            .db
            .set_setting(SETTINGS_KEY, &serde_json::to_string(settings)?)
    }

    /// Root directory for every engine's weights and Hugging Face caches.
    /// Prefers the big storage volume when present so multi-gigabyte models
    /// stay off the system drive; the Models page can override it.
    fn models_root(&self) -> PathBuf {
        if let Some(root) = self.settings().models_root {
            return root;
        }
        let big_storage = PathBuf::from("/mnt/Big storage");
        if big_storage.is_dir() {
            big_storage.join("AudiobookGen/models")
        } else {
            self.core.data_dir.join("models")
        }
    }

    fn engine_model_dir(&self, engine: &str) -> PathBuf {
        let subdir = match engine {
            "maya1" => "maya1",
            "voxtral" => "voxtral-4b-tts",
            _ => "kokoro-82m",
        };
        let configured = self.models_root().join(subdir);
        if engine == "kokoro" && !kokoro_installed(&configured) {
            // Installs from before the configurable models root live under the
            // app data directory; keep using them instead of re-downloading.
            let legacy = self.core.data_dir.join("models/kokoro-82m");
            if kokoro_installed(&legacy) {
                return legacy;
            }
        }
        configured
    }

    fn engine_options(&self, engine: &str) -> Value {
        let settings = self.settings();
        match engine {
            "maya1" => serde_json::json!({
                "quant": settings.maya1_quant,
                "device": settings.maya1_device,
                "temperature": settings.maya1_temperature,
            }),
            "voxtral" => serde_json::json!({
                "profile": settings.voxtral_profile,
                "seed": settings.voxtral_seed,
            }),
            _ => serde_json::json!({}),
        }
    }

    /// Worker with the dependency extras this engine needs installed. If the
    /// install added new packages, the running worker is restarted so it can
    /// import them.
    async fn worker_for_engine(&self, engine: &str) -> Result<Arc<WorkerSupervisor>> {
        let extras: &[&str] = match engine {
            "maya1" => &["maya1"],
            "voxtral" => &["voxtral"],
            _ => &[],
        };
        let mut slot = self.worker.lock().await;
        let environment_changed = self.ensure_worker_environment(extras).await?;
        if environment_changed {
            if let Some(worker) = slot.take() {
                let _ = worker.shutdown().await;
            }
        }
        if let Some(worker) = slot.as_ref() {
            return Ok(worker.clone());
        }
        let args = vec!["-m".to_owned(), "audiobookgen_worker.main".to_owned()];
        let worker = Arc::new(WorkerSupervisor::spawn(&self.python, &args, &self.worker_root).await
            .with_context(|| format!("The narration worker could not start. Install the worker environment or set AUDIOBOOKGEN_PYTHON. Python: {}", self.python.display()))?);
        worker
            .ping()
            .await
            .context("The narration worker did not answer its startup check")?;
        *slot = Some(worker.clone());
        Ok(worker)
    }

    async fn ensure_worker_environment(&self, extras: &[&str]) -> Result<bool> {
        if self.bootstrap_python.is_none() {
            return Ok(false);
        }
        let bootstrap = self.bootstrap_python.clone().expect("checked above");
        let python = self.python.clone();
        let worker_root = self.worker_root.clone();
        let requested_extras: Vec<String> = extras.iter().map(|extra| extra.to_string()).collect();
        tokio::task::spawn_blocking(move || -> Result<bool> {
            let venv_dir = python
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| anyhow!("invalid managed Python path"))?
                .to_path_buf();
            // The marker records the interpreter version, a hash of the worker's
            // pyproject, and which dependency extras are installed. A missing
            // marker means the last install never finished; a stale hash or a
            // newly requested extra means packages must be (re)installed. The
            // venv itself is only rebuilt when absent.
            let marker = venv_dir.join(".audiobookgen-install-ok");
            let stamp = worker_install_stamp(&worker_root)?;
            let (installed_stamp, installed_extras, installed_accelerator) =
                std::fs::read_to_string(&marker)
                .map(|content| parse_install_marker(content.trim()))
                .unwrap_or_default();
            let mut wanted_extras: Vec<String> = installed_extras.clone();
            for extra in &requested_extras {
                if !wanted_extras.contains(extra) {
                    wanted_extras.push(extra.clone());
                }
            }
            wanted_extras.sort();
            let wanted_accelerator = if wanted_extras.iter().any(|extra| extra == "maya1")
                && cuda_build_available()
            {
                "cuda"
            } else {
                "cpu"
            };
            let needs_cuda_build =
                wanted_accelerator == "cuda" && !llama_cuda_available(&python);
            if python.exists()
                && installed_stamp == stamp
                && installed_extras == wanted_extras
                && installed_accelerator == wanted_accelerator
            {
                return Ok(false);
            }
            let install_spec = if wanted_extras.is_empty() {
                worker_root.to_string_lossy().into_owned()
            } else {
                format!("{}[{}]", worker_root.to_string_lossy(), wanted_extras.join(","))
            };
            let expected_marker =
                format_install_marker(&stamp, &wanted_extras, wanted_accelerator);
            if !python.exists() && venv_dir.exists() {
                std::fs::remove_dir_all(&venv_dir)
                    .context("removing the stale managed Python environment")?;
            }
            std::fs::create_dir_all(venv_dir.parent().unwrap_or(&venv_dir))?;
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
                if !python.exists() {
                    let status = std::process::Command::new(&uv)
                        .args(["venv", "--python", WORKER_PYTHON_VERSION])
                        .arg(&venv_dir)
                        .status()
                        .context("creating the managed Python environment with uv")?;
                    if !status.success() {
                        bail!("uv venv failed with {status}");
                    }
                }
                let status = std::process::Command::new(&uv)
                    .args(["pip", "install", "--python"])
                    .arg(&python)
                    .args(["--torch-backend", "auto"])
                    .arg(&install_spec)
                    .status()
                    .context("installing the narration worker dependencies with uv")?;
                if !status.success() {
                    bail!("uv pip install failed with {status}");
                }
                if needs_cuda_build {
                    let status = std::process::Command::new(&uv)
                        .env("CMAKE_ARGS", "-DGGML_CUDA=on")
                        .env("FORCE_CMAKE", "1")
                        .args(["pip", "install", "--python"])
                        .arg(&python)
                        .args([
                            "--upgrade",
                            "--force-reinstall",
                            "--no-cache-dir",
                            "llama-cpp-python>=0.3.2",
                        ])
                        .status()
                        .context("building llama-cpp-python with CUDA support")?;
                    if !status.success() {
                        bail!("the CUDA build of llama-cpp-python failed with {status}");
                    }
                }
                std::fs::write(&marker, &expected_marker)?;
                return Ok(true);
            }
            if !python.exists() {
                let status = std::process::Command::new(&bootstrap)
                    .args(["-m", "venv"])
                    .arg(&venv_dir)
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
            }
            let status = std::process::Command::new(&python)
                .args([
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "--no-input",
                ])
                .arg(&install_spec)
                .status()
                .context("installing the narration worker dependencies")?;
            if !status.success() {
                bail!("installing the narration worker failed with {status}");
            }
            if needs_cuda_build {
                let status = std::process::Command::new(&python)
                    .env("CMAKE_ARGS", "-DGGML_CUDA=on")
                    .env("FORCE_CMAKE", "1")
                    .args([
                        "-m",
                        "pip",
                        "install",
                        "--upgrade",
                        "--force-reinstall",
                        "--no-cache-dir",
                        "llama-cpp-python>=0.3.2",
                    ])
                    .status()
                    .context("building llama-cpp-python with CUDA support")?;
                if !status.success() {
                    bail!("the CUDA build of llama-cpp-python failed with {status}");
                }
            }
            std::fs::write(&marker, &expected_marker)?;
            Ok(true)
        })
        .await?
    }

    fn cache_dir(&self) -> PathBuf {
        self.core.data_dir.join("cache/segments")
    }

    fn worker_accelerator(&self) -> Option<String> {
        let marker = self
            .python
            .parent()
            .and_then(Path::parent)?
            .join(".audiobookgen-install-ok");
        std::fs::read_to_string(marker)
            .ok()
            .map(|content| parse_install_marker(content.trim()).2)
    }

    fn worker_extra_installed(&self, extra: &str) -> bool {
        let marker = match self.python.parent().and_then(Path::parent) {
            Some(parent) => parent.join(".audiobookgen-install-ok"),
            None => return false,
        };
        std::fs::read_to_string(marker)
            .ok()
            .map(|content| parse_install_marker(content.trim()).1)
            .is_some_and(|extras| extras.iter().any(|installed| installed == extra))
    }
}

fn kokoro_installed(model_dir: &Path) -> bool {
    (model_dir.join("config.json").exists() && model_dir.join("kokoro-v1_0.pth").exists())
        || model_dir.join("MOCK_MODEL").exists()
}

fn maya1_installed(model_dir: &Path, quant: &str) -> bool {
    model_dir.join(format!("maya1.{quant}.gguf")).exists() || model_dir.join("MOCK_MODEL").exists()
}

fn voxtral_installed(model_dir: &Path) -> bool {
    (model_dir.join("consolidated.safetensors").exists()
        && model_dir.join("tekken.json").exists()
        && model_dir.join("voice_embedding").is_dir())
        || model_dir.join("MOCK_MODEL").exists()
}

// Kokoro's dependency chain supports >=3.10,<3.14; keep this inside that range.
const WORKER_PYTHON_VERSION: &str = "3.12";

/// Identity of a finished worker install: interpreter version plus a hash of the
/// worker's pyproject, so dependency changes trigger a reinstall on upgrade.
fn format_install_marker(stamp: &str, extras: &[String], accelerator: &str) -> String {
    format!(
        "{stamp}|extras={}|accelerator={accelerator}",
        extras.join(",")
    )
}

fn parse_install_marker(content: &str) -> (String, Vec<String>, String) {
    match content.split_once("|extras=") {
        Some((stamp, tail)) => {
            let (extras, accelerator) = tail
                .split_once("|accelerator=")
                .map_or((tail, "cpu"), |(extras, accelerator)| (extras, accelerator));
            (
                stamp.to_owned(),
                extras
                    .split(',')
                    .filter(|extra| !extra.is_empty())
                    .map(str::to_owned)
                    .collect(),
                accelerator.to_owned(),
            )
        }
        // Markers from before extras support carry the bare stamp.
        None => (content.to_owned(), Vec::new(), "cpu".into()),
    }
}

fn cuda_build_available() -> bool {
    if voxtral::detect_nvidia_gpu().is_none() {
        return false;
    }
    std::process::Command::new("nvcc")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn llama_cuda_available(python: &Path) -> bool {
    python.exists()
        && std::process::Command::new(python)
            .args([
                "-c",
                "import llama_cpp; raise SystemExit(0 if llama_cpp.llama_supports_gpu_offload() else 1)",
            ])
            .status()
            .is_ok_and(|status| status.success())
}

fn worker_install_stamp(worker_root: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let pyproject = std::fs::read(worker_root.join("pyproject.toml"))
        .context("reading the worker pyproject for the install stamp")?;
    let digest = hex::encode(Sha256::digest(&pyproject));
    Ok(format!("{WORKER_PYTHON_VERSION}:{digest}"))
}

fn file_sha256(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut source = std::fs::File::open(path)
        .with_context(|| format!("opening checksum input {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 8 * 1024 * 1024];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn voxtral_cache_discriminator(
    model_dir: &Path,
    voice: &str,
    settings: &AppSettings,
) -> Result<String> {
    let model_checksum = file_sha256(&model_dir.join("consolidated.safetensors"))?;
    let voice_checksum = file_sha256(
        &model_dir
            .join("voice_embedding")
            .join(format!("{voice}.pt")),
    )?;
    let flow_steps = if settings.voxtral_profile == "quality" {
        8
    } else {
        3
    };
    Ok(format!(
        "model={model_checksum}|voice={voice_checksum}|engine=voxtral-int4-93d3e21|quant=hqq-int4-g64-tile-packed-to-4d|profile={}|flow_steps={flow_steps}|cfg=1.2|seed={}|postprocess=lowpass-resample-peak-v1|sample_rate=48000",
        settings.voxtral_profile, settings.voxtral_seed
    ))
}

fn extend_cache_key(base: &str, discriminator: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(base.as_bytes());
    digest.update([0]);
    digest.update(discriminator.as_bytes());
    hex::encode(digest.finalize())
}

fn managed_python(data_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        data_dir.join("worker-venv/Scripts/python.exe")
    } else {
        data_dir.join("worker-venv/bin/python")
    }
}

const KOKORO_VOICES: &[&str] = &[
    "af_alloy",
    "af_aoede",
    "af_bella",
    "af_heart",
    "af_jessica",
    "af_kore",
    "af_nicole",
    "af_nova",
    "af_river",
    "af_sarah",
    "af_sky",
    "am_adam",
    "am_echo",
    "am_eric",
    "am_fenrir",
    "am_liam",
    "am_michael",
    "am_onyx",
    "am_puck",
    "am_santa",
    "bf_alice",
    "bf_emma",
    "bf_isabella",
    "bf_lily",
    "bm_daniel",
    "bm_fable",
    "bm_george",
    "bm_lewis",
];

fn validate_voice(engine: &str, voice: &str) -> Result<()> {
    match engine {
        "kokoro" => {
            if !KOKORO_VOICES.contains(&voice) {
                bail!("unsupported Kokoro voice");
            }
        }
        // Maya1 voices are free-form natural-language descriptions.
        "maya1" => {
            if voice.trim().is_empty() || voice.len() > 500 {
                bail!("maya1 needs a voice description of up to 500 characters");
            }
        }
        // Voxtral preset names are validated by the server; new presets must
        // keep working without an app update.
        "voxtral" => {
            if voice.trim().is_empty() || voice.len() > 80 {
                bail!("voxtral needs a preset voice name");
            }
        }
        other => bail!("unknown narration engine: {other}"),
    }
    Ok(())
}

fn engine_model_revision(engine: &str) -> &'static str {
    match engine {
        "maya1" => "mradermacher/maya1-GGUF",
        "voxtral" => "mistralai/Voxtral-4B-TTS-2603",
        _ => "hexgrad/Kokoro-82M",
    }
}

fn command_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn emit_model_progress(
    app: &AppHandle,
    engine: &str,
    phase: &str,
    message: impl Into<String>,
    current: Option<u64>,
    total: Option<u64>,
) {
    let _ = app.emit(
        "model-progress",
        ModelProgress {
            engine: engine.into(),
            phase: phase.into(),
            message: message.into(),
            current,
            total,
        },
    );
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
        engine: default_engine(),
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
    validate_voice(&input.engine, &input.voice).map_err(command_error)?;
    if !(0.5..=2.0).contains(&input.speed) {
        return Err("speed must be between 0.5 and 2.0".into());
    }
    if input.engine == "voxtral" && (input.speed - 1.0).abs() > f32::EPSILON {
        return Err("Voxtral currently supports narration speed 1.0 only".into());
    }
    let profile = NarrationProfile {
        id: Uuid::new_v4(),
        book_id,
        name: input.name.trim().into(),
        model_revision: engine_model_revision(&input.engine).into(),
        engine: input.engine,
        voice: input.voice,
        speed: input.speed,
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
pub async fn get_app_settings(
    runtime: tauri::State<'_, AppRuntime>,
) -> std::result::Result<AppSettings, String> {
    Ok(runtime.settings())
}

#[tauri::command]
pub async fn update_app_settings(
    runtime: tauri::State<'_, AppRuntime>,
    settings: AppSettings,
) -> std::result::Result<AppSettings, String> {
    if !["Q8_0", "Q6_K", "Q4_K_M"].contains(&settings.maya1_quant.as_str()) {
        return Err("unsupported maya1 quantization".into());
    }
    if !["auto", "cpu"].contains(&settings.maya1_device.as_str()) {
        return Err("maya1 device must be auto or cpu".into());
    }
    if !(0.1..=1.5).contains(&settings.maya1_temperature) {
        return Err("maya1 temperature must be between 0.1 and 1.5".into());
    }
    if !["balanced", "quality", "compatibility"].contains(&settings.voxtral_profile.as_str()) {
        return Err("unsupported Voxtral quality profile".into());
    }
    if let Some(root) = &settings.models_root {
        if !root.is_absolute() {
            return Err("the models folder must be an absolute path".into());
        }
        std::fs::create_dir_all(root)
            .map_err(|error| format!("the models folder is not writable: {error}"))?;
    }
    runtime.save_settings(&settings).map_err(command_error)?;
    Ok(settings)
}

#[tauri::command]
pub async fn list_engine_status(
    runtime: tauri::State<'_, AppRuntime>,
) -> std::result::Result<Vec<EngineModelStatus>, String> {
    let settings = runtime.settings();
    Ok(ENGINES
        .iter()
        .map(|&engine| {
            let path = runtime.engine_model_dir(engine);
            match engine {
                "maya1" => EngineModelStatus {
                    engine: engine.into(),
                    installed: maya1_installed(&path, &settings.maya1_quant),
                    path,
                    quant: Some(settings.maya1_quant.clone()),
                    runtime_installed: None,
                    voices: Vec::new(),
                    accelerator_message: Some(
                        match runtime.worker_accelerator().as_deref() {
                            Some("cuda") => "CUDA acceleration is installed for Maya1.".into(),
                            _ if cuda_build_available() => "An NVIDIA GPU and CUDA toolkit were detected; the next Maya1 install will build CUDA acceleration automatically.".into(),
                            _ => "CUDA was not detected; Maya1 will use its CPU fallback.".into(),
                        },
                    ),
                    hardware_supported: None,
                },
                "voxtral" => EngineModelStatus {
                    engine: engine.into(),
                    installed: voxtral_installed(&path),
                    voices: std::fs::read_dir(path.join("voice_embedding"))
                        .ok()
                        .into_iter()
                        .flatten()
                        .filter_map(|entry| entry.ok())
                        .filter_map(|entry| {
                            (entry.path().extension().and_then(|value| value.to_str()) == Some("pt"))
                                .then(|| entry.path().file_stem()?.to_str().map(str::to_owned))
                                .flatten()
                        })
                        .collect(),
                    path,
                    quant: None,
                    runtime_installed: Some(runtime.worker_extra_installed("voxtral")),
                    accelerator_message: Some(voxtral::cuda_status_message()),
                    hardware_supported: Some(
                        voxtral::detect_nvidia_gpu().is_some_and(|gpu| gpu.can_run_voxtral()),
                    ),
                },
                _ => EngineModelStatus {
                    engine: engine.into(),
                    installed: kokoro_installed(&path),
                    path,
                    quant: None,
                    runtime_installed: None,
                    voices: Vec::new(),
                    accelerator_message: None,
                    hardware_supported: None,
                },
            }
        })
        .collect())
}

#[tauri::command]
pub async fn download_engine_model(
    app: AppHandle,
    runtime: tauri::State<'_, AppRuntime>,
    engine: String,
) -> std::result::Result<Value, String> {
    if !ENGINES.contains(&engine.as_str()) {
        return Err("unknown narration engine".into());
    }
    if engine == "voxtral" && !runtime.settings().voxtral_license_accepted {
        return Err(
            "Accept the Voxtral model's CC BY-NC 4.0 license in Models before downloading.".into(),
        );
    }
    let stage = |phase: &str, message: &str| {
        emit_model_progress(&app, &engine, phase, message, None, None);
    };
    stage(
        "preparing",
        "Preparing the narration engine. The first run installs Python packages and can take several minutes.",
    );
    let worker = runtime.worker_for_engine(&engine).await.map_err(|error| {
        stage("failed", "The narration engine could not be prepared.");
        command_error(error)
    })?;
    stage(
        "downloading",
        match engine.as_str() {
            "maya1" => "Downloading Maya1 weights (about 3.4 GB for Q8_0) and the SNAC decoder.",
            "voxtral" => "Downloading Voxtral 4B TTS weights (about 9 GB).",
            _ => "Downloading the Kokoro model (about 330 MB).",
        },
    );
    let progress_app = app.clone();
    let progress_engine = engine.clone();
    let response = worker
        .request_with_progress(
            WorkerRequest::DownloadModel {
                id: Uuid::new_v4().to_string(),
                engine: engine.clone(),
                model_dir: runtime.engine_model_dir(&engine),
                options: runtime.engine_options(&engine),
            },
            move |response| {
                let state = response.state.as_deref().unwrap_or("downloading");
                let message = match (response.current, response.total) {
                    (Some(current), Some(total)) if total > 0 => format!(
                        "Downloading model files: {:.1}%",
                        current as f64 / total as f64 * 100.0
                    ),
                    _ => format!("Model setup: {state}"),
                };
                emit_model_progress(
                    &progress_app,
                    &progress_engine,
                    state,
                    message,
                    response.current,
                    response.total,
                );
            },
        )
        .await
        .map_err(|error| {
            stage("failed", "The model download failed.");
            command_error(error)
        })?;
    stage(
        "complete",
        "The model is ready. Voxtral will start automatically when it is used.",
    );
    Ok(response.payload)
}

#[tauri::command]
pub async fn preview_voice(
    app: AppHandle,
    runtime: tauri::State<'_, AppRuntime>,
    text: String,
    engine: String,
    voice: String,
    speed: f32,
) -> std::result::Result<String, String> {
    validate_voice(&engine, &voice).map_err(command_error)?;
    ensure_model(runtime.inner(), &engine).map_err(command_error)?;
    let output = runtime
        .core
        .data_dir
        .join("cache")
        .join(format!("preview-{}.wav", Uuid::new_v4()));
    let worker = runtime
        .worker_for_engine(&engine)
        .await
        .map_err(command_error)?;
    let progress_app = app.clone();
    let progress_engine = engine.clone();
    worker
        .request_with_progress(
            WorkerRequest::Generate {
                id: Uuid::new_v4().to_string(),
                engine: engine.clone(),
                text,
                voice,
                speed,
                output_path: output.clone(),
                model_dir: runtime.engine_model_dir(&engine),
                options: runtime.engine_options(&engine),
            },
            move |response| {
                emit_model_progress(
                    &progress_app,
                    &progress_engine,
                    response.state.as_deref().unwrap_or("generating"),
                    response
                        .state
                        .as_deref()
                        .unwrap_or("Generating voice preview"),
                    response.current,
                    response.total,
                );
            },
        )
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

#[tauri::command]
pub async fn stop_voxtral_runtime(
    runtime: tauri::State<'_, AppRuntime>,
) -> std::result::Result<(), String> {
    let worker = runtime.worker.lock().await.take();
    if let Some(worker) = worker {
        worker.shutdown().await.map_err(command_error)?;
    }
    Ok(())
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
    ensure_model(runtime, &profile.engine)?;
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
    let worker = runtime.worker_for_engine(&profile.engine).await?;
    let voxtral_discriminator = if profile.engine == "voxtral" {
        let model_dir = runtime.engine_model_dir("voxtral");
        let voice = profile.voice.clone();
        let settings = runtime.settings();
        Some(
            tokio::task::spawn_blocking(move || {
                voxtral_cache_discriminator(&model_dir, &voice, &settings)
            })
            .await??,
        )
    } else {
        None
    };
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
        if let Some(discriminator) = &voxtral_discriminator {
            fragment.cache_key = extend_cache_key(&fragment.cache_key, discriminator);
        }
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
        let mut word_timings = Vec::new();
        let (duration_ms, sample_rate) = if fragment.kind == FragmentKind::SceneBreak {
            let sample_rate = if profile.engine == "voxtral" {
                48_000
            } else {
                24_000
            };
            write_silence(&temporary, 40, sample_rate)?;
            (40, sample_rate)
        } else {
            let progress_app = app.clone();
            let progress_job_id = job_id.to_owned();
            let progress_book_id = request.book_id.to_string();
            let progress_fragment_id = fragment.id.to_string();
            let response = worker
                .request_with_progress(
                    WorkerRequest::Generate {
                        id: Uuid::new_v4().to_string(),
                        engine: profile.engine.clone(),
                        text: fragment.spoken_text.clone(),
                        voice: profile.voice.clone(),
                        speed: profile.speed,
                        output_path: temporary.clone(),
                        model_dir: runtime.engine_model_dir(&profile.engine),
                        options: runtime.engine_options(&profile.engine),
                    },
                    move |response| {
                        let state = response.state.as_deref().unwrap_or("generating");
                        let _ = progress_app.emit(
                            "generation-progress",
                            GenerationProgress {
                                job_id: progress_job_id.clone(),
                                book_id: progress_book_id.clone(),
                                completed,
                                total,
                                fragment_id: Some(progress_fragment_id.clone()),
                                state: "generating",
                                message: Some(format!("Voxtral worker: {state}")),
                            },
                        );
                    },
                )
                .await?;
            word_timings = response.word_timings.clone();
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
            word_timings,
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

fn write_silence(path: &Path, duration_ms: u64, sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec)?;
    for _ in 0..duration_ms * u64::from(sample_rate) / 1_000 {
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

#[tauri::command]
pub async fn get_generated_segment(
    runtime: tauri::State<'_, AppRuntime>,
    fragment_id: String,
    profile_id: String,
) -> std::result::Result<Option<GeneratedSegment>, String> {
    Ok(runtime
        .core
        .db
        .generated_segment(
            parse_uuid(&fragment_id, "fragment id").map_err(command_error)?,
            parse_uuid(&profile_id, "profile id").map_err(command_error)?,
        )
        .map_err(command_error)?
        .filter(|segment| segment.audio_path.exists()))
}

fn ensure_model(runtime: &AppRuntime, engine: &str) -> Result<()> {
    let model_dir = runtime.engine_model_dir(engine);
    let installed = match engine {
        "maya1" => maya1_installed(&model_dir, &runtime.settings().maya1_quant),
        "voxtral" => voxtral_installed(&model_dir),
        _ => kokoro_installed(&model_dir),
    };
    if installed {
        Ok(())
    } else {
        bail!("The {engine} model is not installed yet. Download it from the Models page first.")
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
