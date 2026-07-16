import ePub, { type Book, type Rendition } from "epubjs";
import { mediaUrl } from "./tauri";
import type { Fragment, FragmentLocator } from "./types";

export type ReaderFlow = "paginated" | "scrolled-doc";

const HIGHLIGHT_STYLE = "background: rgba(245, 230, 168, 0.9); border-radius: 2px;";

interface FragmentText {
  id: string;
  source_text: string;
}

export interface FragmentTextRange {
  fragmentId: string;
  start: number;
  end: number;
}

function compactText(text: string): { value: string; offsets: number[] } {
  let value = "";
  const offsets: number[] = [];
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (/\s/u.test(character)) continue;
    value += character.toLowerCase();
    offsets.push(index);
  }
  return { value, offsets };
}

export function locateFragmentRanges(blockText: string, fragments: FragmentText[]): FragmentTextRange[] {
  const block = compactText(blockText);
  const ranges: FragmentTextRange[] = [];
  let cursor = 0;

  for (const fragment of fragments) {
    const source = compactText(fragment.source_text).value;
    if (!source) continue;
    const compactStart = block.value.indexOf(source, cursor);
    if (compactStart < 0) continue;
    const compactEnd = compactStart + source.length;
    ranges.push({
      fragmentId: fragment.id,
      start: block.offsets[compactStart],
      end: block.offsets[compactEnd - 1] + 1,
    });
    cursor = compactEnd;
  }

  return ranges;
}

interface RelocatedLocation {
  start: { href: string; cfi: string };
}

export class EpubReader {
  private book: Book | null = null;
  private rendition: Rendition | null = null;
  private fragments: Fragment[] = [];
  private onFragmentClick: ((fragmentId: string) => void) | null = null;
  private fragmentElements = new Map<string, HTMLElement[]>();
  private currentFragmentId: string | null = null;
  private destroyed = false;

  async open(sourcePath: string, container: HTMLElement, flow: ReaderFlow, onLocation: (locator: FragmentLocator) => void): Promise<void> {
    const response = await fetch(mediaUrl(sourcePath));
    if (!response.ok) throw new Error(`could not read the EPUB file (${response.status})`);
    const data = await response.arrayBuffer();
    if (this.destroyed) return;
    const book = ePub(data);
    this.book = book;
    await book.ready;
    if (this.destroyed) { book.destroy(); this.book = null; return; }
    const rendition = book.renderTo(container, {
      flow,
      width: Math.max(container.clientWidth, 1),
      height: Math.max(container.clientHeight, 1),
      allowScriptedContent: false,
    });
    this.rendition = rendition;
    rendition.on("relocated", (location: RelocatedLocation) => {
      onLocation({ href: location.start.href, cfi: location.start.cfi, css_selector: null, text_occurrence: 0, source_text_hash: "" });
    });
    rendition.on("rendered", () => this.bindFragments());
    await rendition.display();
  }

  setFragments(fragments: Fragment[], onClick: (fragmentId: string) => void): void {
    const previousChapterId = this.fragments[0]?.chapter_id;
    const nextChapterId = fragments[0]?.chapter_id;
    if (previousChapterId && previousChapterId !== nextChapterId) this.clearFragmentBindings();
    this.fragments = fragments;
    this.onFragmentClick = onClick;
    this.bindFragments();
  }

  setCurrent(fragmentId: string | null): void {
    if (this.currentFragmentId) {
      for (const previous of this.fragmentElements.get(this.currentFragmentId) ?? []) {
        previous.removeAttribute("data-abg-current");
        previous.style.background = "";
        previous.style.borderRadius = "";
      }
    }
    this.currentFragmentId = fragmentId;
    if (!fragmentId) return;
    const elements = this.fragmentElements.get(fragmentId) ?? [];
    for (const element of elements) {
      element.setAttribute("data-abg-current", "true");
      element.style.cssText += HIGHLIGHT_STYLE;
    }
    elements[0]?.scrollIntoView({ block: "center", behavior: "smooth" });
  }

  async goTo(locator: FragmentLocator): Promise<void> {
    if (!this.rendition) return;
    const target = locator.cfi ?? locator.href;
    if (!target) return;
    try { await this.rendition.display(target); } catch { /* stale locators must not break reading */ }
  }

  async next(): Promise<void> { await this.rendition?.next(); }
  async previous(): Promise<void> { await this.rendition?.prev(); }

  destroy(): void {
    this.destroyed = true;
    this.fragmentElements.clear();
    this.rendition = null;
    this.book?.destroy();
    this.book = null;
  }

  private clearFragmentBindings(): void {
    if (!this.rendition) return;
    const contents = this.rendition.getContents() as unknown as Array<{ document: Document }>;
    for (const { document } of contents) {
      for (const marker of document.querySelectorAll<HTMLElement>("[data-abg-fragment]")) {
        const parent = marker.parentNode;
        marker.replaceWith(document.createTextNode(marker.textContent ?? ""));
        parent?.normalize();
      }
      delete document.body?.dataset.abgBound;
    }
    this.fragmentElements.clear();
  }

  private bindFragments(): void {
    if (!this.rendition || !this.fragments.length) return;
    const contents = this.rendition.getContents() as unknown as Array<{ document: Document; sectionIndex: number }>;
    const boundFragments = new Set(this.fragmentElements.keys());
    for (const content of contents) {
      const document = content.document;
      if (!document || document.body.dataset.abgBound === "true") continue;
      if (document.body.dataset.abgClickBound !== "true") {
        document.body.dataset.abgClickBound = "true";
        document.body.addEventListener("click", (event) => {
          const marker = (event.target as Element | null)?.closest<HTMLElement>("[data-abg-fragment]");
          const fragmentId = marker?.dataset.abgFragment;
          if (!fragmentId) return;
          event.stopPropagation();
          this.onFragmentClick?.(fragmentId);
        });
      }
      const blocks = Array.from(document.body.querySelectorAll<HTMLElement>("p, h1, h2, h3, h4, h5, h6, li, blockquote, figcaption, td, th"));
      for (const block of blocks) {
        const text = block.textContent ?? "";
        if (!text) continue;
        const candidates = this.fragments.filter((fragment) => !boundFragments.has(fragment.id));
        const ranges = locateFragmentRanges(text, candidates);
        if (!ranges.length) continue;

        const walker = document.createTreeWalker(block, document.defaultView?.NodeFilter.SHOW_TEXT ?? 4);
        const textNodes: Array<{ node: Text; start: number; end: number }> = [];
        let textOffset = 0;
        for (let node = walker.nextNode(); node; node = walker.nextNode()) {
          const textNode = node as Text;
          const end = textOffset + textNode.data.length;
          textNodes.push({ node: textNode, start: textOffset, end });
          textOffset = end;
        }

        const slices = textNodes.flatMap(({ node, start, end }) => ranges.flatMap((range) => {
          const sliceStart = Math.max(start, range.start);
          const sliceEnd = Math.min(end, range.end);
          return sliceStart < sliceEnd
            ? [{ node, start: sliceStart - start, end: sliceEnd - start, fragmentId: range.fragmentId }]
            : [];
        }));

        for (const slice of slices.toSorted((left, right) => right.start - left.start)) {
          const selected = slice.node.splitText(slice.start);
          selected.splitText(slice.end - slice.start);
          const marker = document.createElement("span");
          marker.dataset.abgFragment = slice.fragmentId;
          marker.style.cursor = "pointer";
          selected.parentNode?.replaceChild(marker, selected);
          marker.append(selected);
          const elements = this.fragmentElements.get(slice.fragmentId) ?? [];
          elements.push(marker);
          this.fragmentElements.set(slice.fragmentId, elements);
          boundFragments.add(slice.fragmentId);
        }
      }
      document.body.dataset.abgBound = "true";
    }
    if (this.currentFragmentId) this.setCurrent(this.currentFragmentId);
  }
}
