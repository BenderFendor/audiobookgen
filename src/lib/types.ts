export type EpubLayout = "reflowable" | "fixed" | "mixed";
export type FootnoteMode = "skip" | "inline" | "end_of_chapter";
export type TableMode = "skip" | "summary" | "cells";
export type CaptionMode = "skip" | "read";
export type FragmentKind = "heading" | "sentence" | "dialogue" | "caption" | "table" | "footnote" | "scene_break";
export type GenerationMode = "current_and_next" | "full_book" | "selected_chapters";

export interface BookSummary {
  id: string;
  title: string;
  authors: string[];
  language: string | null;
  cover_path: string | null;
  source_path: string;
  layout: EpubLayout;
  chapter_count: number;
  generated_sentences: number;
  total_sentences: number;
  active_profile_id: string | null;
  created_at: string;
  updated_at: string;
}

export interface ChapterReview {
  index: number;
  title: string;
  href: string;
  media_type: string;
  layout: EpubLayout;
  selected: boolean;
  estimated_words: number;
  footnote_count: number;
  caption_count: number;
  table_count: number;
  warnings: string[];
}

export interface ImportReview {
  source_path: string;
  source_sha256: string;
  title: string;
  authors: string[];
  language: string | null;
  publisher: string | null;
  description: string | null;
  identifier: string | null;
  layout: EpubLayout;
  drm_detected: boolean;
  chapters: ChapterReview[];
  cover_entry: string | null;
  warnings: string[];
}

export interface ImportSelection {
  selected_chapter_indices: number[];
  footnote_mode: FootnoteMode;
  table_mode: TableMode;
  caption_mode: CaptionMode;
}

export interface Chapter {
  id: string;
  book_id: string;
  index: number;
  title: string;
  href: string;
  media_type: string;
  layout: EpubLayout;
  selected: boolean;
  fragment_count: number;
}

export interface FragmentLocator {
  href: string;
  css_selector: string | null;
  text_occurrence: number;
  source_text_hash: string;
  cfi: string | null;
}

export interface Fragment {
  id: string;
  book_id: string;
  chapter_id: string;
  chapter_index: number;
  index: number;
  source_text: string;
  spoken_text: string;
  kind: FragmentKind;
  locator: FragmentLocator;
  pause_after_ms: number;
  cache_key: string;
}

export type TtsEngine = "kokoro" | "maya1" | "voxtral";

export interface NarrationProfile {
  id: string;
  book_id: string;
  name: string;
  engine: TtsEngine;
  voice: string;
  speed: number;
  model_revision: string;
  model_sha256: string | null;
  normalization_version: string;
  planner_version: string;
  created_at: string;
}

export interface CreateNarrationProfile {
  name: string;
  engine: TtsEngine;
  voice: string;
  speed: number;
}

export interface BookDetail {
  summary: BookSummary;
  chapters: Chapter[];
  profiles: NarrationProfile[];
}

export interface QueueGeneration {
  book_id: string;
  profile_id: string;
  mode: GenerationMode;
  current_chapter_index: number | null;
  selected_chapter_indices: number[];
}

export interface ProgressState {
  book_id: string;
  reading_locator: FragmentLocator | null;
  listening_fragment_id: string | null;
  listening_offset_ms: number;
  linked: boolean;
  updated_at?: string | null;
}

export interface PronunciationRule {
  id: string;
  book_id: string | null;
  pattern: string;
  replacement: string;
  case_sensitive: boolean;
  created_at: string;
}

export interface EngineModelStatus {
  engine: TtsEngine;
  installed: boolean;
  path: string;
  quant: string | null;
  server_reachable: boolean | null;
  server_url: string | null;
}

export interface AppSettings {
  models_root: string | null;
  maya1_quant: string;
  maya1_device: string;
  maya1_temperature: number;
  voxtral_server_url: string;
  voxtral_default_voice: string;
}

export interface WordTiming {
  word: string;
  start_ms: number;
  end_ms: number;
}

export interface GeneratedSegment {
  fragment_id: string;
  profile_id: string;
  cache_key: string;
  audio_path: string;
  duration_ms: number;
  sample_rate: number;
  word_timings: WordTiming[];
  created_at: string;
}

export type GenerationState = "running" | "generating" | "complete" | "failed" | "cancelled";

export interface GenerationProgress {
  jobId: string;
  bookId: string;
  completed: number;
  total: number;
  fragmentId: string | null;
  state: GenerationState;
  message: string | null;
}
