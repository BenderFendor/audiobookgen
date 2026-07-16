import { create } from "zustand";
import type { BookDetail, BookSummary, GenerationProgress, ImportReview } from "./types";

interface AppState {
  books: BookSummary[];
  activeBook: BookDetail | null;
  importReview: ImportReview | null;
  generation: GenerationProgress | null;
  error: string | null;
  setBooks: (books: BookSummary[]) => void;
  setActiveBook: (book: BookDetail | null) => void;
  setImportReview: (review: ImportReview | null) => void;
  setGeneration: (progress: GenerationProgress | null) => void;
  setError: (error: string | null) => void;
}

export const useAppStore = create<AppState>((set) => ({
  books: [],
  activeBook: null,
  importReview: null,
  generation: null,
  error: null,
  setBooks: (books) => set({ books }),
  setActiveBook: (activeBook) => set({ activeBook }),
  setImportReview: (importReview) => set({ importReview }),
  setGeneration: (generation) => set({ generation }),
  setError: (error) => set({ error }),
}));
