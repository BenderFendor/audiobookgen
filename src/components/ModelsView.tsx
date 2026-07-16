"use client";

import { useMemo, useState } from "react";
import { api, wavObjectUrl } from "@/lib/tauri";
import { ENGINE_LABELS, KOKORO_VOICES, VOXTRAL_PRESET_VOICES, kokoroVoiceLabel } from "@/lib/voices";
import type { AppSettings, EngineModelStatus, ModelProgress, TtsEngine } from "@/lib/types";

const PREVIEW_TEXT = "The quick brown fox jumps over the lazy dog, and the audiobook begins.";

const MAYA1_QUANTS = [
  ["Q8_0", "Q8_0 · 3.4 GB · near-lossless (recommended)"],
  ["Q6_K", "Q6_K · 2.6 GB · slightly smaller, minimal loss"],
  ["Q4_K_M", "Q4_K_M · 2.0 GB · fastest, small quality loss"],
] as const;

interface Props {
  engines: EngineModelStatus[];
  settings: AppSettings;
  modelProgress: ModelProgress | null;
  onSettingsChange: (settings: AppSettings) => Promise<void>;
  onDownload: (engine: TtsEngine) => Promise<void>;
  onStopVoxtral: () => Promise<void>;
  onError: (message: string) => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024 ** 2) return `${Math.round(bytes / 1024)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

export function ModelsView({ engines, settings, modelProgress, onSettingsChange, onDownload, onStopVoxtral, onError }: Props) {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [busyEngine, setBusyEngine] = useState<TtsEngine | null>(null);
  const [previewBusy, setPreviewBusy] = useState<TtsEngine | null>(null);
  const [previewVoice, setPreviewVoice] = useState<Record<TtsEngine, string>>({
    kokoro: "af_heart",
    maya1: "Adult narrator, neutral accent, warm and clear",
    voxtral: settings.voxtral_default_voice || "neutral_female",
  });
  const [savingSettings, setSavingSettings] = useState(false);
  const status = useMemo(() => new Map(engines.map((engine) => [engine.engine, engine])), [engines]);
  const voxtral = status.get("voxtral");

  const saveSettings = async () => {
    setSavingSettings(true);
    try { await onSettingsChange(draft); } catch (error) { onError(String(error)); } finally { setSavingSettings(false); }
  };

  const download = async (engine: TtsEngine) => {
    setBusyEngine(engine);
    try {
      if (engine === "voxtral") await onSettingsChange(draft);
      await onDownload(engine);
    } catch (error) { onError(String(error)); } finally { setBusyEngine(null); }
  };

  const playPreview = async (engine: TtsEngine) => {
    setPreviewBusy(engine);
    try {
      const path = await api.previewVoice(PREVIEW_TEXT, engine, previewVoice[engine], 1.0);
      const url = await wavObjectUrl(path);
      const audio = new Audio(url);
      audio.onended = () => URL.revokeObjectURL(url);
      await audio.play();
    } catch (error) { onError(String(error)); } finally { setPreviewBusy(null); }
  };

  const stopVoxtral = async () => {
    try { await onStopVoxtral(); } catch (error) { onError(String(error)); }
  };

  const progressPercent = modelProgress?.current != null && modelProgress.total
    ? Math.min(100, (modelProgress.current / modelProgress.total) * 100)
    : null;

  return (
    <main className="models-page">
      <section className="library-hero">
        <div>
          <p className="eyebrow">NARRATION MODELS</p>
          <h1>Download, tune, and test every narrator engine.</h1>
          <p className="hero-copy">Models and caches are stored in one folder you control. Each narrator profile picks one of these engines.</p>
        </div>
      </section>

      <section className="models-root card">
        <h2>Storage</h2>
        <label className="field">Models folder
          <input
            value={draft.models_root ?? ""}
            placeholder="/mnt/Big storage/AudiobookGen/models"
            onChange={(event) => setDraft({ ...draft, models_root: event.target.value || null })}
          />
        </label>
        <small>Weights and Hugging Face caches land here. Already downloaded models are not moved when this changes.</small>
        <button className="secondary-button" disabled={savingSettings} onClick={() => void saveSettings()}>Save storage and engine settings</button>
        {modelProgress && (
          <div className="model-progress" aria-live="polite">
            <div><strong>{ENGINE_LABELS[modelProgress.engine]}</strong><span>{modelProgress.message}</span></div>
            <progress value={progressPercent ?? undefined} max={100} />
            {progressPercent !== null && modelProgress.current !== null && modelProgress.total !== null && (
              <small>{progressPercent.toFixed(1)}% · {formatBytes(modelProgress.current)} of {formatBytes(modelProgress.total)}</small>
            )}
          </div>
        )}
      </section>

      <section className="model-cards">
        <article className="card model-card">
          <header>
            <h2>{ENGINE_LABELS.kokoro}</h2>
            <span className={status.get("kokoro")?.installed ? "status-dot ready" : "status-dot"} />
          </header>
          <p>Fast local narration with 28 English voices and true per-word timing for read-along highlighting. About 330 MB; runs comfortably on CPU or GPU.</p>
          <small className="model-path">{status.get("kokoro")?.path}</small>
          {!status.get("kokoro")?.installed && (
            <button className="primary-button" disabled={busyEngine !== null} onClick={() => void download("kokoro")}>
              {busyEngine === "kokoro" ? "Installing…" : "Download Kokoro (330 MB)"}
            </button>
          )}
          <div className="preview-row">
            <select value={previewVoice.kokoro} onChange={(event) => setPreviewVoice({ ...previewVoice, kokoro: event.target.value })}>
              {KOKORO_VOICES.map((voice) => <option key={voice} value={voice}>{kokoroVoiceLabel(voice)}</option>)}
            </select>
            <button disabled={!status.get("kokoro")?.installed || previewBusy !== null} onClick={() => void playPreview("kokoro")}>
              {previewBusy === "kokoro" ? "Synthesizing…" : "Preview voice"}
            </button>
          </div>
        </article>

        <article className="card model-card">
          <header>
            <h2>{ENGINE_LABELS.maya1}</h2>
            <span className={status.get("maya1")?.installed ? "status-dot ready" : "status-dot"} />
          </header>
          <p>Voice-design narration: describe the narrator in plain English ("70-year-old male, gravelly, slow") and use inline emotion tags like &lt;laugh&gt; or &lt;sigh&gt;. Runs quantized GGUF weights through llama.cpp with the SNAC decoder.</p>
          <small className="model-path">{status.get("maya1")?.path}</small>
          <label className="field">Quantization
            <select value={draft.maya1_quant} onChange={(event) => setDraft({ ...draft, maya1_quant: event.target.value })}>
              {MAYA1_QUANTS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}
            </select>
          </label>
          <label className="field">Compute device
            <select value={draft.maya1_device} onChange={(event) => setDraft({ ...draft, maya1_device: event.target.value })}>
              <option value="auto">Auto (GPU when available)</option>
              <option value="cpu">CPU only</option>
            </select>
          </label>
          <label className="field">Temperature · {draft.maya1_temperature.toFixed(2)}
            <input type="range" min="0.1" max="1.5" step="0.05" value={draft.maya1_temperature}
              onChange={(event) => setDraft({ ...draft, maya1_temperature: Number(event.target.value) })} />
          </label>
          <small>0.4 is the reference default. Lower is steadier; higher is more expressive but less consistent between sentences. When an NVIDIA GPU and CUDA toolkit are present, AudiobookGen builds llama.cpp with CUDA automatically; otherwise it keeps the CPU fallback.</small>
          {status.get("maya1")?.accelerator_message && <p className="accelerator-state">{status.get("maya1")?.accelerator_message}</p>}
          {!status.get("maya1")?.installed && (
            <button className="primary-button" disabled={busyEngine !== null} onClick={() => void download("maya1")}>
              {busyEngine === "maya1" ? "Installing…" : `Download Maya1 ${draft.maya1_quant}`}
            </button>
          )}
          <div className="preview-row">
            <input value={previewVoice.maya1} placeholder="Describe the narrator voice"
              onChange={(event) => setPreviewVoice({ ...previewVoice, maya1: event.target.value })} />
            <button disabled={!status.get("maya1")?.installed || previewBusy !== null} onClick={() => void playPreview("maya1")}>
              {previewBusy === "maya1" ? "Synthesizing…" : "Preview voice"}
            </button>
          </div>
        </article>

        <article className="card model-card">
          <header>
            <h2>{ENGINE_LABELS.voxtral}</h2>
            <span className={voxtral?.installed && voxtral?.runtime_installed ? "status-dot ready" : "status-dot"} />
          </header>
          <p>Mistral's lifelike TTS adapted for a 12 GB NVIDIA GPU. AudiobookGen selectively quantizes the language backbone to HQQ INT4 while keeping its acoustic transformer and codec in BF16. The existing local narration worker owns the model, so no server or terminal setup is required.</p>
          <small className="model-path">{voxtral?.path}</small>
          <label className="field">Quality profile
            <select value={draft.voxtral_profile} onChange={(event) => setDraft({ ...draft, voxtral_profile: event.target.value as AppSettings["voxtral_profile"] })}>
              <option value="balanced">Balanced · 3 flow steps · CFG 1.2 · compiled</option>
              <option value="quality">Quality · 8 flow steps · CFG 1.2</option>
              <option value="compatibility">Compatibility · 3 flow steps · CFG 1.2 · no compile</option>
            </select>
          </label>
          <small>Every production profile retains text-conditioning CFG. Compatibility mode is the fallback after a verified compilation failure; it does not reduce narration quality by disabling CFG.</small>
          {voxtral?.accelerator_message && <p className="accelerator-state">{voxtral.accelerator_message}</p>}
          <label className="license-consent">
            <input type="checkbox" checked={draft.voxtral_license_accepted} onChange={(event) => setDraft({ ...draft, voxtral_license_accepted: event.target.checked })} />
            <span>I accept the Voxtral model and reference voices under CC BY-NC 4.0. The adapted inference code is MIT; the weights are not licensed for commercial use.</span>
          </label>
          <p className="server-state">INT4 dependencies: {voxtral?.runtime_installed ? "installed" : "not installed"} · Weights: {voxtral?.installed ? "installed" : "not installed"}</p>
          {(!voxtral?.installed || !voxtral?.runtime_installed) && (
            <button className="primary-button" disabled={busyEngine !== null || !draft.voxtral_license_accepted || voxtral?.hardware_supported === false} onClick={() => void download("voxtral")}>
              {busyEngine === "voxtral" ? "Installing…" : "Install Voxtral INT4 + CUDA runtime"}
            </button>
          )}
          {voxtral?.runtime_installed && (
            <button className="secondary-button" onClick={() => void stopVoxtral()}>Stop narration worker and free VRAM</button>
          )}
          <div className="preview-row">
            <select value={previewVoice.voxtral} onChange={(event) => setPreviewVoice({ ...previewVoice, voxtral: event.target.value })}>
              {(voxtral?.voices.length ? voxtral.voices : VOXTRAL_PRESET_VOICES).map((voice) => <option key={voice} value={voice}>{voice.replace(/_/g, " ")}</option>)}
            </select>
            <button disabled={!voxtral?.installed || previewBusy !== null} onClick={() => void playPreview("voxtral")}>
              {previewBusy === "voxtral" ? "Synthesizing…" : "Preview voice"}
            </button>
          </div>
        </article>
      </section>
    </main>
  );
}
