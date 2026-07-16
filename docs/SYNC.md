# Accountless sync package

The first sync implementation intentionally uses ordinary folders. A destination can be a USB drive, an SMB/NFS mount, Syncthing folder, Nextcloud folder, or any other filesystem path already available to the operating system.

```text
<chosen-folder>/AudiobookGen/<book-id>/
├── <book-id>-<profile-id>.epub
└── manifest.json
```

The EPUB is a complete narrated EPUB 3 package containing text, chapter audio, and Media Overlays. `manifest.json` identifies the book, narration profile, format version, filename, and latest progress event.

The manifest is versioned so a future Android client and self-hosted server can consume the same package. Future network sync should exchange immutable content-addressed files plus progress events rather than defining a second book format.

## Conflict rule for future clients

Progress should be resolved by semantic position and event time, not by whichever device connects last. A client must retain both reading and listening state, compare monotonic event timestamps, and never discard a newer offline event without recording a conflict. Highlights and annotations should receive stable IDs and merge as an append-only event log.

The current desktop command performs an atomic-enough folder handoff for local use: it finishes the narrated EPUB before copying it and writes the manifest after the book file is present.
