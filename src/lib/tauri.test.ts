import { afterEach, describe, expect, it } from "vitest";
import { isTauri, mediaUrl } from "./tauri";

afterEach(() => {
  delete (globalThis as Record<string, unknown>).window;
});

describe("isTauri", () => {
  it("is false outside a browser window", () => {
    expect(isTauri()).toBe(false);
  });
  it("is false in a plain browser without the Tauri bridge", () => {
    (globalThis as Record<string, unknown>).window = {};
    expect(isTauri()).toBe(false);
  });
  it("is true when the Tauri internals are injected", () => {
    (globalThis as Record<string, unknown>).window = { __TAURI_INTERNALS__: {} };
    expect(isTauri()).toBe(true);
  });
});

describe("mediaUrl", () => {
  it("passes paths through untouched outside Tauri so the web preview still renders", () => {
    expect(mediaUrl("/covers/book.jpg")).toBe("/covers/book.jpg");
  });
});
