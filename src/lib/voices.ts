import type { TtsEngine, WordTiming } from "./types";

export const ENGINE_LABELS: Record<TtsEngine, string> = {
  kokoro: "Kokoro 82M",
  maya1: "Maya1 3B",
  voxtral: "Voxtral 4B TTS",
};

// Every English voice shipped with Kokoro v1.0.
export const KOKORO_VOICES = [
  "af_alloy", "af_aoede", "af_bella", "af_heart", "af_jessica", "af_kore", "af_nicole",
  "af_nova", "af_river", "af_sarah", "af_sky", "am_adam", "am_echo", "am_eric", "am_fenrir",
  "am_liam", "am_michael", "am_onyx", "am_puck", "am_santa", "bf_alice", "bf_emma",
  "bf_isabella", "bf_lily", "bm_daniel", "bm_fable", "bm_george", "bm_lewis",
] as const;

export const VOXTRAL_PRESET_VOICES = [
  "casual_male", "casual_female", "calm_male", "calm_female",
  "narrator_male", "narrator_female", "upbeat_male", "upbeat_female",
] as const;

export function kokoroVoiceLabel(voice: string): string {
  const [prefix, name] = voice.split("_");
  if (!prefix || !name) return voice;
  const region = prefix.startsWith("a") ? "US" : "UK";
  const gender = prefix.endsWith("f") ? "female" : "male";
  return `${name[0].toUpperCase()}${name.slice(1)} · ${region} ${gender}`;
}

export function voxtralVoiceLabel(voice: string): string {
  return voice.replace(/_/g, " ");
}

/** Short human summary of a profile's voice for the narrator dropdown. */
export function voiceSummary(engine: TtsEngine, voice: string): string {
  if (engine === "kokoro") return kokoroVoiceLabel(voice);
  if (engine === "voxtral") return voxtralVoiceLabel(voice);
  return voice.length > 42 ? `${voice.slice(0, 42)}…` : voice;
}

/**
 * Length-proportional fallback used when an engine reports no word timings.
 * Mirrors the worker-side estimator so highlighting stays consistent.
 */
export function estimateWordTimings(text: string, durationMs: number): WordTiming[] {
  const words = text.split(/\s+/u).filter(Boolean);
  if (!words.length || durationMs <= 0) return [];
  const weights = words.map((word) => word.length + 2);
  const total = weights.reduce((sum, weight) => sum + weight, 0);
  const timings: WordTiming[] = [];
  let cursor = 0;
  for (let index = 0; index < words.length; index += 1) {
    const start = cursor;
    cursor += (durationMs * weights[index]) / total;
    timings.push({ word: words[index], start_ms: Math.round(start), end_ms: Math.round(cursor) });
  }
  return timings;
}

/**
 * Which display word to highlight at a playback position. When the timing list
 * and the displayed word count disagree (engine tokenization differences), the
 * index is scaled so the highlight still tracks the sentence.
 */
export function wordIndexAt(timings: WordTiming[], positionMs: number, displayWordCount: number): number {
  if (!timings.length || displayWordCount <= 0) return -1;
  let index = -1;
  for (let cursor = 0; cursor < timings.length; cursor += 1) {
    if (timings[cursor].start_ms <= positionMs) index = cursor;
    else break;
  }
  if (index < 0) return -1;
  if (timings.length === displayWordCount) return index;
  return Math.min(displayWordCount - 1, Math.floor((index * displayWordCount) / timings.length));
}
