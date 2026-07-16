import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  BookDetail, BookSummary, CreateNarrationProfile, Fragment, GenerationProgress,
  ImportReview, ImportSelection, ModelStatus, NarrationProfile, ProgressState,
  PronunciationRule, QueueGeneration,
} from "./types";

export function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function mediaUrl(path: string): string {
  return isTauri() ? convertFileSrc(path) : path;
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
  modelStatus: () => invoke<ModelStatus>("model_status"),
  downloadModel: () => invoke<unknown>("download_model"),
  previewVoice: (text: string, voice: string, speed: number) => invoke<string>("preview_voice", { text, voice, speed }),
  queueGeneration: (request: QueueGeneration) => invoke<string>("queue_generation", { request }),
  cancelGeneration: (jobId: string) => invoke<void>("cancel_generation", { jobId }),
  generatedAudio: (fragmentId: string, profileId: string) => invoke<string | null>("get_generated_audio", { fragmentId, profileId }),
  exportM4a: (bookId: string, profileId: string, outputDir: string) => invoke<string[]>("export_m4a", { bookId, profileId, outputDir }),
  exportM4b: (bookId: string, profileId: string, outputPath: string) => invoke<string>("export_m4b_file", { bookId, profileId, outputPath }),
  exportNarratedEpub: (bookId: string, profileId: string, outputPath: string) => invoke<string>("export_narrated_epub_file", { bookId, profileId, outputPath }),
  syncToFolder: (bookId: string, profileId: string, folder: string) => invoke<string>("sync_to_folder", { bookId, profileId, folder }),
};

export async function onGenerationProgress(callback: (progress: GenerationProgress) => void): Promise<() => void> {
  return listen<GenerationProgress>("generation-progress", (event) => callback(event.payload));
}
