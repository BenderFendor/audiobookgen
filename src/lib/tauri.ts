import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  AppSettings, BookDetail, BookSummary, CreateNarrationProfile, EngineModelStatus, Fragment,
  GeneratedSegment, GenerationProgress, ImportReview, ImportSelection, NarrationProfile,
  ModelProgress, ProgressState, PronunciationRule, QueueGeneration, TtsEngine,
} from "./types";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function mediaUrl(path: string): string {
  return isTauri() ? convertFileSrc(path) : path;
}

export async function wavObjectUrl(path: string): Promise<string> {
  const response = await fetch(mediaUrl(path));
  if (!response.ok) throw new Error(`could not read generated audio (${response.status})`);
  const bytes = await response.arrayBuffer();
  return URL.createObjectURL(new Blob([bytes], { type: "audio/wav" }));
}

export const api = {
  inspectEpub: (path: string) => invoke<ImportReview>("inspect_epub_file", { path }),
  importEpub: (review: ImportReview, selection: ImportSelection) => invoke<BookDetail>("import_epub", { review, selection }),
  listBooks: () => invoke<BookSummary[]>("list_books"),
  getBook: (bookId: string) => invoke<BookDetail | null>("get_book", { bookId }),
  getFragments: (chapterId: string) => invoke<Fragment[]>("get_chapter_fragments", { chapterId }),
  createProfile: (bookId: string, input: CreateNarrationProfile) => invoke<NarrationProfile>("create_narration_profile", { bookId, input }),
  setActiveProfile: (bookId: string, profileId: string) => invoke<void>("set_active_profile", { bookId, profileId }),
  savePronunciationRule: (bookId: string, pattern: string, replacement: string, caseSensitive = false) =>
    invoke<PronunciationRule>("save_pronunciation_rule", { bookId, pattern, replacement, caseSensitive }),
  saveProgress: (progress: ProgressState) => invoke<void>("save_progress", { progress }),
  loadProgress: (bookId: string) => invoke<ProgressState | null>("load_progress", { bookId }),
  listEngineStatus: () => invoke<EngineModelStatus[]>("list_engine_status"),
  downloadEngineModel: (engine: TtsEngine) => invoke<unknown>("download_engine_model", { engine }),
  stopVoxtralRuntime: () => invoke<void>("stop_voxtral_runtime"),
  getAppSettings: () => invoke<AppSettings>("get_app_settings"),
  updateAppSettings: (settings: AppSettings) => invoke<AppSettings>("update_app_settings", { settings }),
  previewVoice: (text: string, engine: TtsEngine, voice: string, speed: number) => invoke<string>("preview_voice", { text, engine, voice, speed }),
  queueGeneration: (request: QueueGeneration) => invoke<string>("queue_generation", { request }),
  anchorGeneration: (bookId: string, profileId: string, fragmentId: string) => invoke<string>("anchor_generation", { bookId, profileId, fragmentId }),
  setGenerationAnchor: (fragmentId: string) => invoke<void>("set_generation_anchor", { fragmentId }),
  cancelGeneration: (jobId: string) => invoke<void>("cancel_generation", { jobId }),
  generatedAudio: (fragmentId: string, profileId: string) => invoke<string | null>("get_generated_audio", { fragmentId, profileId }),
  generatedSegment: (fragmentId: string, profileId: string) => invoke<GeneratedSegment | null>("get_generated_segment", { fragmentId, profileId }),
  exportM4a: (bookId: string, profileId: string, outputDir: string) => invoke<string[]>("export_m4a", { bookId, profileId, outputDir }),
  exportM4b: (bookId: string, profileId: string, outputPath: string) => invoke<string>("export_m4b_file", { bookId, profileId, outputPath }),
  exportNarratedEpub: (bookId: string, profileId: string, outputPath: string) => invoke<string>("export_narrated_epub_file", { bookId, profileId, outputPath }),
  syncToFolder: (bookId: string, profileId: string, folder: string) => invoke<string>("sync_to_folder", { bookId, profileId, folder }),
};

export async function onGenerationProgress(callback: (progress: GenerationProgress) => void): Promise<() => void> {
  return listen<GenerationProgress>("generation-progress", (event) => callback(event.payload));
}

export async function onModelProgress(callback: (progress: ModelProgress) => void): Promise<() => void> {
  return listen<ModelProgress>("model-progress", (event) => callback(event.payload));
}
