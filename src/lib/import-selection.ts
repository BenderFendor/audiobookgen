import type { ImportReview, ImportSelection } from "./types";

export function defaultImportSelection(review: ImportReview): ImportSelection {
  return {
    selected_chapter_indices: review.chapters.filter((chapter) => chapter.selected).map((chapter) => chapter.index),
    footnote_mode: "skip",
    table_mode: "summary",
    caption_mode: "read",
  };
}

export function toggleChapter(selection: ImportSelection, index: number): ImportSelection {
  const selected = selection.selected_chapter_indices.includes(index)
    ? selection.selected_chapter_indices.filter((value) => value !== index)
    : [...selection.selected_chapter_indices, index].sort((a, b) => a - b);
  return { ...selection, selected_chapter_indices: selected };
}
