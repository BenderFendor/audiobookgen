"use client";

import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { ImportReviewPanel } from "./ImportReviewPanel";
import { LibraryView } from "./LibraryView";
import { ReaderStudio } from "./ReaderStudio";
import { api, isTauri, onGenerationProgress } from "@/lib/tauri";
import { useAppStore } from "@/lib/store";
import type { ImportSelection, ModelStatus } from "@/lib/types";

export function AppShell() {
  const { books, activeBook, importReview, generation, error, setBooks, setActiveBook, setImportReview, setGeneration, setError } = useAppStore();
  const [model, setModel] = useState<ModelStatus | null>(null);
  const [modelBusy, setModelBusy] = useState(false);

  const refreshBooks = useCallback(async () => {
    if (!isTauri()) return;
    setBooks(await api.listBooks());
  }, [setBooks]);

  const refreshActive = useCallback(async () => {
    if (!activeBook) return;
    const next = await api.getBook(activeBook.summary.id);
    setActiveBook(next);
    await refreshBooks();
  }, [activeBook, refreshBooks, setActiveBook]);

  useEffect(() => {
    if (!isTauri()) return;
    refreshBooks().catch((reason) => setError(String(reason)));
    api.modelStatus().then(setModel).catch((reason) => setError(String(reason)));
    let unlisten: (() => void) | undefined;
    onGenerationProgress(setGeneration).then((fn) => { unlisten = fn; }).catch((reason) => setError(String(reason)));
    return () => unlisten?.();
  }, [refreshBooks, setBooks, setError, setGeneration]);

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
    try { setActiveBook(await api.getBook(bookId)); } catch (reason) { setError(String(reason)); }
  };

  const installModel = async () => {
    setModelBusy(true);
    setError(null);
    try { await api.downloadModel(); setModel(await api.modelStatus()); } catch (reason) { setError(String(reason)); } finally { setModelBusy(false); }
  };

  return (
    <div className="app-root">
      <header className="global-header"><button className="wordmark" onClick={() => setActiveBook(null)}>Audiobook<span>Gen</span></button><div className="header-status"><span className={model?.installed ? "status-dot ready" : "status-dot"} />{model?.installed ? "Kokoro ready" : "Kokoro model not installed"}{!model?.installed && <button onClick={() => void installModel()} disabled={modelBusy}>{modelBusy ? "Downloading…" : "Download"}</button>}</div><button className="header-import" onClick={() => void chooseEpub()}>+ Import EPUB</button></header>
      {error && <div className="global-error">{error}<button onClick={() => setError(null)}>×</button></div>}
      {activeBook ? <ReaderStudio book={activeBook} generation={generation} onBack={() => setActiveBook(null)} onRefresh={refreshActive} /> : <LibraryView books={books} onOpen={(id) => void openBook(id)} onImport={() => void chooseEpub()} />}
      {importReview && <ImportReviewPanel review={importReview} onCancel={() => setImportReview(null)} onImport={importBook} />}
    </div>
  );
}
