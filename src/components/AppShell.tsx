"use client";

import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { ImportReviewPanel } from "./ImportReviewPanel";
import { LibraryView } from "./LibraryView";
import { ModelsView } from "./ModelsView";
import { ReaderStudio } from "./ReaderStudio";
import { api, isTauri, onGenerationProgress, onModelProgress } from "@/lib/tauri";
import { useAppStore } from "@/lib/store";
import type { AppSettings, EngineModelStatus, ImportSelection, TtsEngine } from "@/lib/types";

export function AppShell() {
  const { books, activeBook, importReview, generation, error, setBooks, setActiveBook, setImportReview, setGeneration, setError } = useAppStore();
  const [view, setView] = useState<"library" | "models">("library");
  const [engines, setEngines] = useState<EngineModelStatus[] | null>(null);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [modelStage, setModelStage] = useState<string | null>(null);

  const refreshBooks = useCallback(async () => {
    if (!isTauri()) return;
    setBooks(await api.listBooks());
  }, [setBooks]);

  const refreshModels = useCallback(async () => {
    if (!isTauri()) return;
    setEngines(await api.listEngineStatus());
    setSettings(await api.getAppSettings());
  }, []);

  const refreshActive = useCallback(async () => {
    if (!activeBook) return;
    const next = await api.getBook(activeBook.summary.id);
    setActiveBook(next);
    await refreshBooks();
  }, [activeBook, refreshBooks, setActiveBook]);

  useEffect(() => {
    if (!isTauri()) return;
    refreshBooks().catch((reason: unknown) => setError(String(reason)));
    refreshModels().catch((reason: unknown) => setError(String(reason)));
    let unlisten: (() => void) | undefined;
    let unlistenModel: (() => void) | undefined;
    onGenerationProgress(setGeneration).then((fn) => { unlisten = fn; }).catch((reason: unknown) => setError(String(reason)));
    onModelProgress(setModelStage).then((fn) => { unlistenModel = fn; }).catch(() => undefined);
    return () => { unlisten?.(); unlistenModel?.(); };
  }, [refreshBooks, refreshModels, setBooks, setError, setGeneration]);

  const chooseEpub = async () => {
    if (!isTauri()) { setError("Run AudiobookGen through the Tauri desktop shell to import local books."); return; }
    const path = await open({ multiple: false, filters: [{ name: "EPUB books", extensions: ["epub"] }] });
    if (typeof path !== "string") return;
    setError(null);
    try { setImportReview(await api.inspectEpub(path)); } catch (reason) { setError(String(reason)); }
  };

  const importBook = async (selection: ImportSelection) => {
    if (!importReview) return;
    const imported = await api.importEpub(importReview, selection);
    setImportReview(null);
    setActiveBook(imported);
    await refreshBooks();
  };

  const openBook = async (bookId: string) => {
    setError(null);
    try {
      setActiveBook(await api.getBook(bookId));
      setView("library");
    } catch (reason) { setError(String(reason)); }
  };

  const downloadEngine = async (engine: TtsEngine) => {
    setError(null);
    try {
      await api.downloadEngineModel(engine);
      setModelStage(null);
    } finally {
      await refreshModels().catch(() => undefined);
    }
  };

  const saveSettings = async (next: AppSettings) => {
    setSettings(await api.updateAppSettings(next));
    await refreshModels();
  };

  const anyInstalled = engines?.some((engine) => engine.installed) ?? true;

  return (
    <div className="app-root">
      <header className="global-header" data-tauri-drag-region>
        <button className="wordmark" onClick={() => { setActiveBook(null); setView("library"); }}>Audiobook<span>Gen</span></button>
        <nav className="header-nav">
          <button className={view === "library" && !activeBook ? "active" : ""} onClick={() => { setActiveBook(null); setView("library"); }}>Library</button>
          <button className={view === "models" && !activeBook ? "active" : ""} onClick={() => { setActiveBook(null); setView("models"); }}>Models</button>
        </nav>
        <div className="header-status">
          <span className={anyInstalled ? "status-dot ready" : "status-dot"} />
          <span className="status-text">
            {engines === null ? "Checking narration models" : anyInstalled ? `${engines.filter((engine) => engine.installed).length} of ${engines.length} models installed` : "No narration model installed"}
          </span>
        </div>
        <button className="header-import" onClick={() => void chooseEpub()}>+ Import EPUB</button>
      </header>
      {error && <div className="global-error">{error}<button onClick={() => setError(null)}>×</button></div>}
      {engines !== null && !anyInstalled && view !== "models" && (
        <div className="model-banner">
          <p>
            <strong>No narration model is installed.</strong>{" "}
            {modelStage ?? "Open the Models page to download Kokoro (fast, 330 MB), Maya1 (voice design), or Voxtral (lifelike presets)."}
          </p>
          <button className="secondary-button" onClick={() => { setActiveBook(null); setView("models"); }}>Open Models</button>
        </div>
      )}
      {activeBook
        ? <ReaderStudio book={activeBook} generation={generation} onBack={() => setActiveBook(null)} onRefresh={refreshActive} />
        : view === "models" && engines && settings
          ? <ModelsView engines={engines} settings={settings} modelStage={modelStage} onSettingsChange={saveSettings} onDownload={downloadEngine} onError={setError} />
          : <LibraryView books={books} onOpen={(id) => void openBook(id)} onImport={() => void chooseEpub()} />}
      {importReview && <ImportReviewPanel review={importReview} onCancel={() => setImportReview(null)} onImport={importBook} />}
    </div>
  );
}
