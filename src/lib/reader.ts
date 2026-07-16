import ePub, { type Book, type Rendition } from "epubjs";
import { mediaUrl } from "./tauri";
import type { Fragment, FragmentLocator } from "./types";

export type ReaderFlow = "paginated" | "scrolled-doc";

const HIGHLIGHT_STYLE = "background: rgba(190, 242, 2, 0.35); border-radius: 2px;";

interface RelocatedLocation {
  start: { href: string; cfi: string };
}

export class EpubReader {
  private book: Book | null = null;
  private rendition: Rendition | null = null;
  private fragments: Fragment[] = [];
  private onFragmentClick: ((fragmentId: string) => void) | null = null;
  private fragmentElements = new Map<string, HTMLElement>();
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
    const rendition = book.renderTo(container, { flow, width: "100%", height: "100%", allowScriptedContent: false });
    this.rendition = rendition;
    rendition.on("relocated", (location: RelocatedLocation) => {
      onLocation({ href: location.start.href, cfi: location.start.cfi, css_selector: null, text_occurrence: 0, source_text_hash: "" });
    });
    rendition.on("rendered", () => this.bindFragments());
    await rendition.display();
  }

  setFragments(fragments: Fragment[], onClick: (fragmentId: string) => void): void {
    this.fragments = fragments;
    this.onFragmentClick = onClick;
    this.bindFragments();
  }

  setCurrent(fragmentId: string | null): void {
    if (this.currentFragmentId) {
      const previous = this.fragmentElements.get(this.currentFragmentId);
      previous?.removeAttribute("data-abg-current");
      if (previous) previous.style.cssText = previous.style.cssText.replace(HIGHLIGHT_STYLE, "");
    }
    this.currentFragmentId = fragmentId;
    if (!fragmentId) return;
    const element = this.fragmentElements.get(fragmentId);
    if (element) {
      element.setAttribute("data-abg-current", "true");
      element.style.cssText += HIGHLIGHT_STYLE;
      element.scrollIntoView({ block: "center", behavior: "smooth" });
    }
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

  private bindFragments(): void {
    if (!this.rendition || !this.fragments.length) return;
    const contents = this.rendition.getContents() as unknown as Array<{ document: Document; sectionIndex: number }>;
    for (const content of contents) {
      const document = content.document;
      if (!document || document.body.dataset.abgBound === "true") continue;
      document.body.dataset.abgBound = "true";
      const blocks = Array.from(document.body.querySelectorAll<HTMLElement>("p, h1, h2, h3, h4, h5, h6, li, blockquote, figcaption, td, th"));
      for (const block of blocks) {
        const text = block.textContent?.trim();
        if (!text) continue;
        const fragment = this.fragments.find((candidate) => text.includes(candidate.source_text.trim()) || candidate.source_text.trim().includes(text));
        if (!fragment) continue;
        if (!this.fragmentElements.has(fragment.id)) this.fragmentElements.set(fragment.id, block);
        block.style.cursor = "pointer";
        block.addEventListener("click", () => this.onFragmentClick?.(fragment.id));
      }
    }
    if (this.currentFragmentId) this.setCurrent(this.currentFragmentId);
  }
}
