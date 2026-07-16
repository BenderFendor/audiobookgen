import { describe, expect, it } from "vitest";
import { defaultImportSelection, toggleChapter } from "./import-selection";
import type { ChapterReview, ImportReview } from "./types";

function chapter(index: number, selected: boolean): ChapterReview {
  return { index, title: `Chapter ${index + 1}`, href: `ch${index}.xhtml`, media_type: "application/xhtml+xml", layout: "reflowable", selected, estimated_words: 1200, footnote_count: 0, caption_count: 0, table_count: 0, warnings: [] };
}

const review: ImportReview = {
  source_path: "/books/sample.epub", source_sha256: "abc", title: "Sample", authors: ["Author"],
  language: "en", publisher: null, description: null, identifier: null, layout: "reflowable",
  drm_detected: false, chapters: [chapter(0, true), chapter(1, false), chapter(2, true)], cover_entry: null, warnings: [],
};

describe("defaultImportSelection", () => {
  it("selects only the chapters the parser marked selected", () => {
    const selection = defaultImportSelection(review);
    expect(selection.selected_chapter_indices).toEqual([0, 2]);
    expect(selection.footnote_mode).toBe("skip");
    expect(selection.table_mode).toBe("summary");
    expect(selection.caption_mode).toBe("read");
  });
});

describe("toggleChapter", () => {
  it("adds an unselected chapter and keeps indices sorted", () => {
    const selection = toggleChapter(defaultImportSelection(review), 1);
    expect(selection.selected_chapter_indices).toEqual([0, 1, 2]);
  });
  it("removes a selected chapter without mutating the input", () => {
    const initial = defaultImportSelection(review);
    const selection = toggleChapter(initial, 0);
    expect(selection.selected_chapter_indices).toEqual([2]);
    expect(initial.selected_chapter_indices).toEqual([0, 2]);
  });
});
