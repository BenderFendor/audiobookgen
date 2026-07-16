"use client";

import { open, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useRef, useState } from "react";
import { EpubReader, type ReaderFlow } from "@/lib/reader";
import { api, wavObjectUrl } from "@/lib/tauri";
import {
  ENGINE_LABELS, KOKORO_VOICES, VOXTRAL_PRESET_VOICES, estimateWordTimings, kokoroVoiceLabel, voiceSummary, wordIndexAt,
} from "@/lib/voices";
import type { BookDetail, CreateNarrationProfile, Fragment, GenerationProgress, NarrationProfile, ProgressState, TtsEngine, WordTiming } from "@/lib/types";

interface Props {
  book: BookDetail;
  generation: GenerationProgress | null;
  onBack: () => void;
  onRefresh: () => Promise<void>;
}

export function ReaderStudio({ book, generation, onBack, onRefresh }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const readerRef = useRef<EpubReader | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const audioObjectUrlRef = useRef<string | null>(null);
  const playbackRequestRef = useRef(0);
  const currentFragmentRef = useRef<string | null>(null);
  const linkedRef = useRef(true);
  const resumeRef = useRef<ProgressState | null>(null);
  const [chapterIndex, setChapterIndex] = useState(book.chapters[0]?.index ?? 0);
  const [fragments, setFragments] = useState<Fragment[]>([]);
  const [currentFragmentId, setCurrentFragmentId] = useState<string | null>(null);
  const [profileId, setProfileId] = useState(book.summary.active_profile_id ?? book.profiles[0]?.id ?? "");
  const [flow, setFlow] = useState<ReaderFlow>("paginated");
  const [linked, setLinked] = useState(true);
  const [jobId, setJobId] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isPlaying, setIsPlaying] = useState(false);
  const [elapsedSeconds, setElapsedSeconds] = useState(0);
  const [durationSeconds, setDurationSeconds] = useState(0);
  const [volume, setVolume] = useState(1);
  const [contextMenu, setContextMenu] = useState<{ fragmentId: string; x: number; y: number } | null>(null);
  const [profileDraft, setProfileDraft] = useState<CreateNarrationProfile | null>(null);
  const wordTimingsRef = useRef<WordTiming[]>([]);
  const activeChapter = useMemo(() => book.chapters.find((chapter) => chapter.index === chapterIndex) ?? book.chapters[0], [book.chapters, chapterIndex]);
  useEffect(() => { currentFragmentRef.current = currentFragmentId; }, [currentFragmentId]);
  useEffect(() => { linkedRef.current = linked; }, [linked]);
  useEffect(() => () => {
    audioRef.current?.pause();
    if (audioObjectUrlRef.current) URL.revokeObjectURL(audioObjectUrlRef.current);
  }, []);

  const activeProfile = useMemo(() => book.profiles.find((profile) => profile.id === profileId) ?? book.profiles[0], [book.profiles, profileId]);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      if (!activeChapter) return;
      const next = await api.getFragments(activeChapter.id);
      if (!cancelled) setFragments(next);
    };
    load().catch((error) => setMessage(String(error)));
    return () => { cancelled = true; };
  }, [activeChapter]);

  useEffect(() => {
    if (!containerRef.current) return;
    const reader = new EpubReader();
    readerRef.current = reader;
    reader.open(book.summary.source_path, containerRef.current, flow, (locator) => {
      const progress: ProgressState = { book_id: book.summary.id, reading_locator: locator, listening_fragment_id: currentFragmentRef.current, listening_offset_ms: Math.round((audioRef.current?.currentTime ?? 0) * 1000), linked: linkedRef.current };
      api.saveProgress(progress).catch(() => undefined);
    }).then(async () => {
      const progress = await api.loadProgress(book.summary.id).catch(() => null);
      if (progress?.reading_locator) await reader.goTo(progress.reading_locator);
      if (progress) {
        const resumedChapter = progress.reading_locator
          ? book.chapters.find((chapter) => chapter.href.split("#")[0] === progress.reading_locator?.href.split("#")[0])
          : null;
        if (resumedChapter) setChapterIndex(resumedChapter.index);
        setLinked(progress.linked);
        setCurrentFragmentId(progress.listening_fragment_id ?? null);
        resumeRef.current = progress;
      }
    }).catch((error) => setMessage(String(error)));
    return () => { reader.destroy(); readerRef.current = null; };
    // Opening a new book or changing flow must rebuild the rendition.
  }, [book.summary.id, book.summary.source_path, flow]);

  useEffect(() => {
    readerRef.current?.setFragments(
      fragments,
      (fragmentId) => { setContextMenu(null); void playFragment(fragmentId); },
      (event) => setContextMenu(event),
    );
  }, [fragments, profileId]);

  useEffect(() => {
    if (!contextMenu) return;
    const dismiss = () => setContextMenu(null);
    window.addEventListener("click", dismiss);
    return () => window.removeEventListener("click", dismiss);
  }, [contextMenu]);

  useEffect(() => {
    readerRef.current?.setCurrent(currentFragmentId);
  }, [currentFragmentId]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target && ["INPUT", "SELECT", "TEXTAREA"].includes(target.tagName)) return;
      if (event.key === "ArrowRight") void readerRef.current?.next();
      else if (event.key === "ArrowLeft") void readerRef.current?.previous();
      else if (event.key === " ") {
        event.preventDefault();
        const audio = audioRef.current;
        if (audio) {
          if (audio.paused) void audio.play();
          else audio.pause();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    if (generation?.bookId !== book.summary.id) return;
    if (generation.state === "complete") {
      setJobId(null);
      setMessage("Narration complete.");
      void onRefresh();
    } else if (generation.state === "failed") {
      setJobId(null);
      setMessage(generation.message ?? "Generation failed.");
    }
  }, [generation, book.summary.id, onRefresh]);

  const playFragment = async (fragmentId: string, options?: { single?: boolean }) => {
    if (!activeProfile) return;
    const requestId = ++playbackRequestRef.current;
    const index = fragments.findIndex((fragment) => fragment.id === fragmentId);
    const segment = await api.generatedSegment(fragmentId, activeProfile.id);
    if (requestId !== playbackRequestRef.current) return;
    if (!segment) {
      setMessage("This sentence is not generated yet. Start generate while reading.");
      return;
    }
    const path = segment.audio_path;
    const fragmentText = fragments[index]?.source_text ?? "";
    wordTimingsRef.current = segment.word_timings.length
      ? segment.word_timings
      : estimateWordTimings(fragmentText, segment.duration_ms);
    audioRef.current?.pause();
    if (audioObjectUrlRef.current) URL.revokeObjectURL(audioObjectUrlRef.current);
    let objectUrl: string;
    try {
      objectUrl = await wavObjectUrl(path);
    } catch (error) {
      setMessage(`Generated audio could not be loaded: ${String(error)}`);
      return;
    }
    if (requestId !== playbackRequestRef.current) {
      URL.revokeObjectURL(objectUrl);
      return;
    }
    audioObjectUrlRef.current = objectUrl;
    const audio = new Audio(objectUrl);
    audio.volume = volume;
    audioRef.current = audio;
    setElapsedSeconds(0);
    setDurationSeconds(0);
    setCurrentFragmentId(fragmentId);
    if (linkedRef.current) await readerRef.current?.goTo(fragments[index]?.locator ?? { href: activeChapter?.href ?? "", cfi: null, css_selector: null, text_occurrence: 0, source_text_hash: "" });
    const resume = resumeRef.current;
    if (resume?.listening_fragment_id === fragmentId && resume.listening_offset_ms > 0) {
      audio.addEventListener("loadedmetadata", () => { audio.currentTime = Math.min(audio.duration || Infinity, resume.listening_offset_ms / 1000); resumeRef.current = null; }, { once: true });
    }
    audio.onloadedmetadata = () => setDurationSeconds(Number.isFinite(audio.duration) ? audio.duration : 0);
    audio.onplay = () => setIsPlaying(true);
    audio.onpause = () => setIsPlaying(false);
    audio.onerror = () => {
      setIsPlaying(false);
      setMessage("The generated sentence could not be played. Check the desktop audio output and restart playback.");
    };
    audio.onended = () => {
      setIsPlaying(false);
      readerRef.current?.setActiveWord(-1);
      if (options?.single) return;
      const next = fragments[index + 1];
      if (next) void playFragment(next.id);
    };
    audio.ontimeupdate = () => {
      setElapsedSeconds(audio.currentTime);
      const reader = readerRef.current;
      if (reader) {
        reader.setActiveWord(wordIndexAt(wordTimingsRef.current, audio.currentTime * 1000, reader.wordCount()));
      }
      api.saveProgress({ book_id: book.summary.id, reading_locator: null, listening_fragment_id: fragmentId, listening_offset_ms: Math.round(audio.currentTime * 1000), linked: linkedRef.current }).catch(() => undefined);
    };
    try {
      await audio.play();
    } catch (error) {
      setMessage(`Playback could not start: ${String(error)}`);
    }
  };

  const togglePlayback = () => {
    const audio = audioRef.current;
    if (audio) {
      if (audio.paused) void audio.play().catch((error) => setMessage(`Playback could not resume: ${String(error)}`));
      else audio.pause();
      return;
    }
    const target = currentFragmentId ?? fragments[0]?.id;
    if (target) void playFragment(target);
  };

  const seek = (seconds: number) => {
    const audio = audioRef.current;
    if (!audio || !Number.isFinite(seconds)) return;
    audio.currentTime = Math.min(Math.max(seconds, 0), audio.duration || seconds);
    setElapsedSeconds(audio.currentTime);
  };

  const changeVolume = (nextVolume: number) => {
    setVolume(nextVolume);
    if (audioRef.current) audioRef.current.volume = nextVolume;
  };

  const generate = async (mode: "current_and_next" | "full_book") => {
    if (!activeProfile) return;
    setBusy("generation");
    setMessage(null);
    try {
      const id = await api.queueGeneration({ book_id: book.summary.id, profile_id: activeProfile.id, mode, current_chapter_index: chapterIndex, selected_chapter_indices: [] });
      setJobId(id);
    } catch (error) { setMessage(String(error)); } finally { setBusy(null); }
  };

  const cancel = async () => {
    if (!jobId) return;
    await api.cancelGeneration(jobId);
    setJobId(null);
  };

  const changeProfile = async (nextProfileId: string) => {
    setProfileId(nextProfileId);
    await api.setActiveProfile(book.summary.id, nextProfileId);
    await onRefresh();
  };

  const submitProfile = async () => {
    if (!profileDraft) return;
    try {
      const profile = await api.createProfile(book.summary.id, profileDraft);
      setProfileDraft(null);
      await onRefresh();
      setProfileId(profile.id);
    } catch (error) { setMessage(String(error)); }
  };

  const draftEngineChanged = (engine: TtsEngine) => {
    setProfileDraft((draft) => draft && {
      ...draft,
      engine,
      voice: engine === "kokoro" ? "af_heart" : engine === "voxtral" ? "narrator_female" : "Adult narrator, neutral accent, warm and clear",
    });
  };

  const correctPronunciation = async () => {
    const fragment = fragments.find((item) => item.id === currentFragmentId);
    if (!fragment) { setMessage("Play or select a sentence first."); return; }
    const pattern = window.prompt("Word or name to correct", fragment.source_text.split(/\s+/).find((word) => word.length > 3) ?? fragment.source_text);
    if (!pattern) return;
    const replacement = window.prompt(`How should “${pattern}” be spoken?`, pattern);
    if (!replacement || replacement === pattern) return;
    await api.savePronunciationRule(book.summary.id, pattern, replacement);
    setMessage("Pronunciation saved. Regenerate this chapter to apply it.");
  };

  const exportBook = async (kind: "m4b" | "epub" | "m4a") => {
    if (!activeProfile) return;
    setBusy(kind);
    setMessage(null);
    try {
      if (kind === "m4a") {
        const folder = await open({ directory: true, multiple: false, title: "Choose chapter export folder" });
        if (typeof folder !== "string") return;
        const paths = await api.exportM4a(book.summary.id, activeProfile.id, folder);
        setMessage(`Exported ${paths.length} chapter M4A files.`);
      } else {
        const extension = kind === "m4b" ? "m4b" : "epub";
        const path = await save({
          title: kind === "m4b" ? "Save chaptered M4B audiobook" : "Save narrated EPUB 3",
          defaultPath: `${book.summary.title}.${extension}`,
          filters: [{ name: kind === "m4b" ? "M4B audiobooks" : "Narrated EPUB books", extensions: [extension] }],
        });
        if (!path) return;
        if (kind === "m4b") {
          await api.exportM4b(book.summary.id, activeProfile.id, path);
        } else {
          await api.exportNarratedEpub(book.summary.id, activeProfile.id, path);
        }
        setMessage("Export complete.");
      }
    } catch (error) { setMessage(String(error)); } finally { setBusy(null); }
  };

  const syncToFolder = async () => {
    if (!activeProfile) return;
    const folder = await open({ directory: true, multiple: false, title: "Choose sync folder" });
    if (typeof folder !== "string") return;
    setBusy("sync");
    setMessage(null);
    try {
      const destination = await api.syncToFolder(book.summary.id, activeProfile.id, folder);
      setMessage(`Copied sync package to ${destination}`);
    } catch (error) { setMessage(String(error)); } finally { setBusy(null); }
  };

  const generating = generation?.bookId === book.summary.id && (generation.state === "running" || generation.state === "generating");
  const percent = generating && generation.total
    ? Math.round((generation.completed / generation.total) * 100)
    : book.summary.total_sentences
      ? Math.round((book.summary.generated_sentences / book.summary.total_sentences) * 100)
      : 0;
  const currentFragment = fragments.find((item) => item.id === currentFragmentId) ?? null;

  return (
    <main className="studio-shell">
      <aside className="studio-sidebar">
        <button className="text-button" onClick={onBack}>← Library</button>
        <div className="studio-book-meta">
          <p className="eyebrow">LOCAL AUDIOBOOK PROJECT</p>
          <h1>{book.summary.title}</h1>
          <p>{book.summary.authors.join(", ") || "Unknown author"}</p>
          <div className="progress-line"><i style={{ width: `${percent}%` }} /></div>
          <small>{percent}% narrated{generating ? ` · ${generation.completed}/${generation.total} sentences` : ""}</small>
        </div>
        <label className="field">Narrator
          <select value={activeProfile?.id ?? ""} onChange={(event) => void changeProfile(event.target.value)}>
            {book.profiles.map((profile: NarrationProfile) => (
              <option key={profile.id} value={profile.id}>{profile.name} · {ENGINE_LABELS[profile.engine]} · {voiceSummary(profile.engine, profile.voice)}</option>
            ))}
          </select>
        </label>
        <button className="text-button" onClick={() => setProfileDraft({ name: "Alternate narrator", engine: "kokoro", voice: "af_heart", speed: 1.0 })}>+ New narrator profile</button>
        <div className="generation-actions">
          <button className="primary-button" disabled={busy === "generation" || generating} onClick={() => void generate("current_and_next")}>Generate while reading</button>
          <button className="secondary-button" disabled={busy === "generation" || generating} onClick={() => void generate("full_book")}>Generate full book</button>
          {(jobId || generating) && <button className="danger-button" onClick={() => void cancel()}>Cancel generation</button>}
        </div>
        <div className="chapter-list" aria-label="Chapters">
          {book.chapters.map((chapter) => (
            <button key={chapter.id} className={chapter.index === chapterIndex ? "active" : ""} onClick={() => {
              setChapterIndex(chapter.index);
              void readerRef.current?.goTo({ href: chapter.href, cfi: null, css_selector: null, text_occurrence: 0, source_text_hash: "" });
            }}>
              <span className="chapter-number">{String(chapter.index + 1).padStart(2, "0")}</span>
              <strong>{chapter.title}</strong>
              <small>{chapter.fragment_count} sentences{chapter.selected ? "" : " · skipped"}</small>
            </button>
          ))}
        </div>
      </aside>
      <section className="reader-column">
        <header className="reader-toolbar">
          <div>
            <button onClick={() => void readerRef.current?.previous()}>←</button>
            <button onClick={() => void readerRef.current?.next()}>→</button>
          </div>
          <div className="segmented">
            <button className={flow === "paginated" ? "active" : ""} onClick={() => setFlow("paginated")}>Pages</button>
            <button className={flow === "scrolled-doc" ? "active" : ""} onClick={() => setFlow("scrolled-doc")}>Scroll</button>
          </div>
          <label className="link-toggle">
            <input type="checkbox" checked={linked} onChange={(event) => setLinked(event.target.checked)} /> Keep reading and listening linked
          </label>
        </header>
        {message && <div className="studio-message">{message}<button onClick={() => setMessage(null)}>×</button></div>}
        <div className="reader-frame" ref={containerRef} />
        {contextMenu && (
          <div className="context-menu" role="menu" style={{ left: contextMenu.x, top: contextMenu.y }}>
            <button role="menuitem" onClick={() => { setContextMenu(null); void playFragment(contextMenu.fragmentId); }}>Listen from here</button>
            <button role="menuitem" onClick={() => { setContextMenu(null); void playFragment(contextMenu.fragmentId, { single: true }); }}>Listen to this sentence</button>
            <button role="menuitem" onClick={() => { setCurrentFragmentId(contextMenu.fragmentId); setContextMenu(null); void correctPronunciation(); }}>Fix pronunciation…</button>
          </div>
        )}
        <footer className="transport-bar">
          <button className="play-button" aria-label={isPlaying ? "Pause narration" : "Play narration"} onClick={togglePlayback}>{isPlaying ? "Ⅱ" : "▶"}</button>
          <div>
            <strong>{activeChapter?.title ?? "No chapter"}</strong>
            <small>{currentFragment?.source_text ?? "Click a sentence in the book to play it."}</small>
            <div className="playback-status">
              <span>{formatTime(elapsedSeconds)}</span>
              <input aria-label="Sentence playback position" type="range" min="0" max={Math.max(durationSeconds, 0.01)} step="0.01" value={Math.min(elapsedSeconds, Math.max(durationSeconds, 0.01))} onChange={(event) => seek(Number(event.target.value))} />
              <span>{formatTime(durationSeconds)}</span>
            </div>
          </div>
          <div className="transport-actions">
            <label>Output {Math.round(volume * 100)}%<input aria-label="Narration output volume" type="range" min="0" max="1" step="0.05" value={volume} onChange={(event) => changeVolume(Number(event.target.value))} /></label>
            <button className="text-button" onClick={() => void correctPronunciation()}>Fix pronunciation</button>
          </div>
        </footer>
      </section>
      <aside className="export-drawer">
        <p className="eyebrow">DELIVER</p>
        <h2>Take the audiobook with you.</h2>
        <button disabled={busy === "m4a"} onClick={() => void exportBook("m4a")}>Chapter M4A files<span>One file per chapter for simple players</span></button>
        <button disabled={busy === "m4b"} onClick={() => void exportBook("m4b")}>Chaptered M4B<span>Single audiobook with chapter marks</span></button>
        <button disabled={busy === "epub"} onClick={() => void exportBook("epub")}>Narrated EPUB 3<span>Read-along book with embedded audio</span></button>
        <button disabled={busy === "sync"} onClick={() => void syncToFolder()}>Sync to folder<span>USB, LAN share, Syncthing, or cloud folder</span></button>
      </aside>
      {profileDraft && (
        <div className="panel-overlay" role="dialog" aria-label="New narrator profile">
          <div className="profile-editor">
            <h2>New narrator profile</h2>
            <label className="field">Name
              <input value={profileDraft.name} onChange={(event) => setProfileDraft({ ...profileDraft, name: event.target.value })} />
            </label>
            <label className="field">Narration model
              <select value={profileDraft.engine} onChange={(event) => draftEngineChanged(event.target.value as TtsEngine)}>
                {(Object.keys(ENGINE_LABELS) as TtsEngine[]).map((engine) => (
                  <option key={engine} value={engine}>{ENGINE_LABELS[engine]}</option>
                ))}
              </select>
            </label>
            {profileDraft.engine === "kokoro" && (
              <label className="field">Voice
                <select value={profileDraft.voice} onChange={(event) => setProfileDraft({ ...profileDraft, voice: event.target.value })}>
                  {KOKORO_VOICES.map((voice) => <option key={voice} value={voice}>{kokoroVoiceLabel(voice)}</option>)}
                </select>
              </label>
            )}
            {profileDraft.engine === "voxtral" && (
              <label className="field">Preset voice
                <select value={profileDraft.voice} onChange={(event) => setProfileDraft({ ...profileDraft, voice: event.target.value })}>
                  {VOXTRAL_PRESET_VOICES.map((voice) => <option key={voice} value={voice}>{voice.replace(/_/g, " ")}</option>)}
                </select>
              </label>
            )}
            {profileDraft.engine === "maya1" && (
              <label className="field">Voice description
                <textarea rows={3} value={profileDraft.voice} placeholder="40-year-old female, low pitch, warm, slight British accent" onChange={(event) => setProfileDraft({ ...profileDraft, voice: event.target.value })} />
              </label>
            )}
            <label className="field">Speed · {profileDraft.speed.toFixed(2)}×{profileDraft.engine === "maya1" ? " (ignored by Maya1)" : ""}
              <input type="range" min="0.5" max="2" step="0.05" value={profileDraft.speed} onChange={(event) => setProfileDraft({ ...profileDraft, speed: Number(event.target.value) })} />
            </label>
            <div className="editor-actions">
              <button className="secondary-button" onClick={() => setProfileDraft(null)}>Cancel</button>
              <button className="primary-button" disabled={!profileDraft.name.trim() || !profileDraft.voice.trim()} onClick={() => void submitProfile()}>Create narrator</button>
            </div>
          </div>
        </div>
      )}
    </main>
  );
}

function formatTime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "0:00";
  const wholeSeconds = Math.floor(seconds);
  return `${Math.floor(wholeSeconds / 60)}:${String(wholeSeconds % 60).padStart(2, "0")}`;
}
