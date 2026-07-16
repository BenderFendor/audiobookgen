import { describe, expect, it } from "vitest";
import { locateFragmentRanges } from "./reader";

describe("locateFragmentRanges", () => {
  it("maps every sentence in a multi-sentence paragraph", () => {
    const text = "The first sentence. The second sentence crosses the page. A third follows.";

    expect(locateFragmentRanges(text, [
      { id: "first", source_text: "The first sentence." },
      { id: "second", source_text: "The second sentence crosses the page." },
      { id: "third", source_text: "A third follows." },
    ])).toEqual([
      { fragmentId: "first", start: 0, end: 19 },
      { fragmentId: "second", start: 20, end: 57 },
      { fragmentId: "third", start: 58, end: 74 },
    ]);
  });

  it("tolerates whitespace introduced around EPUB drop caps and inline markup", () => {
    const text = "THE SOLDIERS ARE WILD and wide-eyed.";

    expect(locateFragmentRanges(text, [
      { id: "drop-cap", source_text: "T HE SOLDIERS ARE WILD and wide-eyed." },
    ])).toEqual([
      { fragmentId: "drop-cap", start: 0, end: text.length },
    ]);
  });

  it("does not bind two fragments to the same text range", () => {
    const text = "Again. Again.";

    expect(locateFragmentRanges(text, [
      { id: "first", source_text: "Again." },
      { id: "second", source_text: "Again." },
    ])).toEqual([
      { fragmentId: "first", start: 0, end: 6 },
      { fragmentId: "second", start: 7, end: 13 },
    ]);
  });
});
