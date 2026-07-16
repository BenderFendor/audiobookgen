import { describe, expect, it } from "vitest";
import { estimateWordTimings, wordIndexAt } from "./voices";

describe("estimateWordTimings", () => {
  it("covers the full duration in order", () => {
    const timings = estimateWordTimings("A reliable test sentence.", 2000);
    expect(timings).toHaveLength(4);
    expect(timings[0].start_ms).toBe(0);
    expect(timings.at(-1)?.end_ms).toBe(2000);
    for (let index = 1; index < timings.length; index += 1) {
      expect(timings[index].start_ms).toBe(timings[index - 1].end_ms);
    }
  });

  it("gives longer words more time", () => {
    const [short, long] = estimateWordTimings("go extraordinary", 1000);
    expect(long.end_ms - long.start_ms).toBeGreaterThan(short.end_ms - short.start_ms);
  });

  it("returns nothing for empty text or duration", () => {
    expect(estimateWordTimings("", 1000)).toHaveLength(0);
    expect(estimateWordTimings("words here", 0)).toHaveLength(0);
  });
});

describe("wordIndexAt", () => {
  const timings = [
    { word: "one", start_ms: 0, end_ms: 400 },
    { word: "two", start_ms: 400, end_ms: 800 },
    { word: "three", start_ms: 800, end_ms: 1200 },
  ];

  it("finds the word containing the position", () => {
    expect(wordIndexAt(timings, 0, 3)).toBe(0);
    expect(wordIndexAt(timings, 450, 3)).toBe(1);
    expect(wordIndexAt(timings, 5000, 3)).toBe(2);
  });

  it("scales indexes when display word count differs", () => {
    expect(wordIndexAt(timings, 900, 6)).toBe(4);
    expect(wordIndexAt(timings, 900, 1)).toBe(0);
  });

  it("handles empty inputs", () => {
    expect(wordIndexAt([], 100, 3)).toBe(-1);
    expect(wordIndexAt(timings, 100, 0)).toBe(-1);
  });
});
