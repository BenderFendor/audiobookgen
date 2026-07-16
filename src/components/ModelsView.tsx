"use client";

import { useMemo, useState } from "react";
import { api, wavObjectUrl } from "@/lib/tauri";
import { ENGINE_LABELS, KOKORO_VOICES, VOXTRAL_PRESET_VOICES, kokoroVoiceLabel } from "@/lib/voices";
import type { AppSettings, EngineModelStatus, TtsEngine } from "@/lib/types";

const PREVIEW_TEXT = "The quick brown fox jumps over the lazy dog, and the audiobook begins.";

const MAYA1_QUANTS = [
  ["Q8_0", "Q8_0 · 3.4 GB · near-lossless (recommended)"],
  ["Q6_K", "Q6_K · 2.6 GB · slightly smaller, minimal loss"],
  ["Q4_K_M", "Q4_K_M · 2.0 GB · fastest, small quality loss"],
] as const;

interface Props {
  engines: EngineModelStatus[];
  settings: AppSettings;
  modelStage: string | null;
  onSettingsChange: (settings: AppSettings) => Promise<void>;
  onDownload: (engine: TtsEngine) => Promise<void>;
  onError: (message: string) => void;
}

export function ModelsView({ engines, settings, modelStage, onSettingsChange, onDownload, onError }: Props) {
  const [draft, setDraft] = useState<AppSettings>(settings);
  const [busyEngine, setBusyEngine] = useState<TtsEngine | null>(null);
  const [previewBusy, setPreviewBusy] = useState<TtsEngine | null>(null);
  const [previewVoice, setPreviewVoice] = useState<Record<TtsEngine, string>>({
    kokoro: "af_heart",
    maya1: "Adult narrator, neutral accent, warm and clear",
    voxtral: settings.voxtral_default_voice || "narrator_female",
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
    try { await onDownload(engine); } catch (error) { onError(String(error)); } finally { setBusyEngine(null); }
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

  const serveCommand = useMemo(() => {
    const modelPath = voxtral?.path ?? "<models folder>/voxtral-4b-tts";
    return [
      "uv venv ~/.venvs/vllm-omni --python 3.12",
      'uv pip install --python ~/.venvs/vllm-omni/bin/python "vllm>=0.18.0" "vllm-omni>=0.18.0" "mistral_common>=1.10.0"',
      `~/.venvs/vllm-omni/bin/vllm serve "${modelPath}" --served-model-name mistralai/Voxtral-4B-TTS-2603 --port 8570 --gpu-memory-utilization 0.90 --max-model-len 4096`,
    ].join("\n");
  }, [voxtral?.path]);

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
        {modelStage && <p className="model-stage">{modelStage}</p>}
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
          <small>0.4 is the reference default. Lower is steadier; higher is more expressive but less consistent between sentences. GPU speed with llama.cpp needs a CUDA build of llama-cpp-python; the default install runs on CPU.</small>
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
            <span className={voxtral?.installed && voxtral?.server_reachable ? "status-dot ready" : "status-dot"} />
          </header>
          <p>Mistral's lifelike TTS with preset voices in nine languages. Voxtral is only supported by vLLM-Omni, so it runs as a separate local server that AudiobookGen talks to. Needs a GPU with roughly 12 GB or more of free VRAM.</p>
          <small className="model-path">{voxtral?.path}</small>
          <label className="field">Server URL
            <input value={draft.voxtral_server_url} onChange={(event) => setDraft({ ...draft, voxtral_server_url: event.target.value })} />
          </label>
          <p className="server-state">
            Server: {voxtral?.server_reachable ? "reachable" : "not reachable"} at {voxtral?.server_url}
          </p>
          {!voxtral?.installed && (
            <button className="primary-button" disabled={busyEngine !== null} onClick={() => void download("voxtral")}>
              {busyEngine === "voxtral" ? "Downloading…" : "Download Voxtral weights (about 9 GB)"}
            </button>
          )}
          <details>
            <summary>How to start the Voxtral server</summary>
            <pre className="serve-command">{serveCommand}</pre>
            <small>Run these once in a terminal after the weights finish downloading. Keep the server running while generating; stop it to free VRAM.</small>
          </details>
          <div className="preview-row">
            <select value={previewVoice.voxtral} onChange={(event) => setPreviewVoice({ ...previewVoice, voxtral: event.target.value })}>
              {VOXTRAL_PRESET_VOICES.map((voice) => <option key={voice} value={voice}>{voice.replace(/_/g, " ")}</option>)}
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
