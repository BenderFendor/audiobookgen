"use client";

import { useMemo, useState } from "react";
import { defaultImportSelection, toggleChapter } from "@/lib/import-selection";
import type { ImportReview, ImportSelection } from "@/lib/types";

interface Props {
  review: ImportReview;
  onCancel: () => void;
  onImport: (selection: ImportSelection) => Promise<void>;
}

export function ImportReviewPanel({ review, onCancel, onImport }: Props) {
  const [selection, setSelection] = useState(() => defaultImportSelection(review));
  const [busy, setBusy] = useState(false);
  const words = useMemo(() => review.chapters.filter((chapter) => selection.selected_chapter_indices.includes(chapter.index)).reduce((sum, chapter) => sum + chapter.estimated_words, 0), [review.chapters, selection.selected_chapter_indices]);
  const submit = async () => {
    setBusy(true);
    try { await onImport(selection); } finally { setBusy(false); }
  };
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-label="Review EPUB import">
      <section className="import-panel">
        <header>
          <div><p className="eyebrow">IMPORT REVIEW</p><h1>{review.title}</h1><p>{review.authors.join(", ") || "Unknown author"} · {review.layout}</p></div>
          <button className="icon-button" onClick={onCancel} aria-label="Close">×</button>
        </header>
        {review.drm_detected && <div className="error-banner">This EPUB contains encrypted resources. AudiobookGen does not remove DRM.</div>}
        {review.warnings.map((warning) => <div className="warning-banner" key={warning}>{warning}</div>)}
        <div className="import-options">
          <label>Footnotes<select value={selection.footnote_mode} onChange={(event) => setSelection({ ...selection, footnote_mode: event.target.value as ImportSelection["footnote_mode"] })}><option value="skip">Skip</option><option value="inline">Read inline</option><option value="end_of_chapter">End of chapter</option></select></label>
          <label>Image captions<select value={selection.caption_mode} onChange={(event) => setSelection({ ...selection, caption_mode: event.target.value as ImportSelection["caption_mode"] })}><option value="read">Read</option><option value="skip">Skip</option></select></label>
          <label>Tables<select value={selection.table_mode} onChange={(event) => setSelection({ ...selection, table_mode: event.target.value as ImportSelection["table_mode"] })}><option value="summary">Read summary</option><option value="cells">Read cells</option><option value="skip">Skip</option></select></label>
        </div>
        <div className="chapter-review-list">
          {review.chapters.map((chapter) => {
            const selected = selection.selected_chapter_indices.includes(chapter.index);
            return <label className={`chapter-review ${selected ? "selected" : ""}`} key={`${chapter.index}-${chapter.href}`}>
              <input type="checkbox" checked={selected} onChange={() => setSelection(toggleChapter(selection, chapter.index))} />
              <span className="chapter-number">{String(chapter.index + 1).padStart(2, "0")}</span>
              <span><strong>{chapter.title}</strong><small>{chapter.estimated_words.toLocaleString()} words · {chapter.layout}{chapter.footnote_count ? ` · ${chapter.footnote_count} notes` : ""}{chapter.table_count ? ` · ${chapter.table_count} tables` : ""}</small></span>
            </label>;
          })}
        </div>
        <footer><p>{selection.selected_chapter_indices.length} chapters · {words.toLocaleString()} words selected</p><div><button className="secondary-button" onClick={onCancel}>Cancel</button><button className="primary-button" disabled={busy || review.drm_detected || !selection.selected_chapter_indices.length} onClick={submit}>{busy ? "Importing…" : "Import book"}</button></div></footer>
      </section>
    </div>
  );
}
