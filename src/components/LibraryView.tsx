"use client";

import { mediaUrl } from "@/lib/tauri";
import type { BookSummary } from "@/lib/types";

interface Props {
  books: BookSummary[];
  onOpen: (bookId: string) => void;
  onImport: () => void;
}

export function LibraryView({ books, onOpen, onImport }: Props) {
  return (
    <main className="library-page">
      <section className="library-hero">
        <div>
          <p className="eyebrow">LOCAL AUDIOBOOK STUDIO</p>
          <h1>Turn an EPUB into a book you can hear.</h1>
          <p className="hero-copy">Kokoro narration, sentence-level read-along, portable M4B and narrated EPUB exports. Your library stays on your machine.</p>
        </div>
        <button className="primary-button" onClick={onImport}>Import EPUB</button>
      </section>
      {books.length === 0 ? (
        <section className="empty-library">
          <span>01</span>
          <h2>No books imported</h2>
          <p>Start with a DRM-free EPUB. You will review chapters, footnotes, captions, and tables before anything is generated.</p>
          <button className="text-button" onClick={onImport}>Choose a book →</button>
        </section>
      ) : (
        <section className="book-grid" aria-label="Audiobook library">
          {books.map((book, position) => {
            const percent = book.total_sentences ? Math.round((book.generated_sentences / book.total_sentences) * 100) : 0;
            return (
              <button className="book-card" key={book.id} onClick={() => onOpen(book.id)}>
                <div className="book-cover">
                  {book.cover_path ? <img src={mediaUrl(book.cover_path)} alt="" /> : <div className="cover-fallback">{String(position + 1).padStart(2, "0")}</div>}
                  <span className="layout-badge">{book.layout.replace("_", " ")}</span>
                </div>
                <div className="book-card-copy">
                  <h2>{book.title}</h2>
                  <p>{book.authors.join(", ") || "Unknown author"}</p>
                  <div className="progress-line"><i style={{ width: `${percent}%` }} /></div>
                  <small>{percent}% narrated · {book.chapter_count} chapters</small>
                </div>
              </button>
            );
          })}
        </section>
      )}
    </main>
  );
}
