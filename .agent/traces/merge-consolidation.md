# Worksheet: merge-consolidation

**Goal:** Consolidate all PRs and agent branches into a single working `main` branch and make the full workspace build and test green.

**What happened:**
- Merged `agent/build-audiobookgen-recovery` (the only branch with a real implementation) into `main`; removed the obsolete `materialize-source` bootstrap workflow.
- `main` did not compile after the merge: `lib.rs` declared `db`, `epub`, and `export` modules whose files were never pushed by the original agent's connector.
- Recovered `db.rs`, `epub.rs`, `export.rs`, `commands.rs`, `epub_pipeline.rs`, the frontend `src/app` and `src/components` files, and `LICENSE` by base64/gzip-decoding the `.bootstrap` payload chunks on `agent/materialize-source` and `agent/build-audiobookgen` (both archives were partially corrupt; salvaged via incremental zlib decompression and manual tar walking).
- `src/lib/{types,tauri,store,reader,import-selection}.ts` existed in **no** payload, branch, or git history. Reconstructed them from their usage surface (components + `commands.rs` command signatures + `model.rs` serde shapes).
- `ReaderStudio.tsx` was corrupt from line 160 onward; kept the intact first half verbatim and reconstructed the export/sync functions and JSX from the CSS class contract in `globals.css`.

**Files changed:** merge of 48-file implementation, plus recovered/reconstructed files above, `Cargo.toml` (zip repin), `rust-toolchain.toml` (1.85→1.88), `.github/workflows/ci.yml` (toolchain bump), `tauri.conf.json` (icon list), `apps/desktop/src-tauri/icons/*` (generated from `src/app/icon.svg`), `src/lib/import-selection.test.ts` (new), `.gitignore` (`gen/`), workspace-wide `cargo fmt`.

**Bugs fixed during verification:**
- `zip = "2.6"` unresolvable: both 2.6.x releases yanked from crates.io; relaxed to `"2"`.
- Rust 1.85 toolchain pin too old for freshly resolved lockfile (darling 0.23 needs 1.88).
- `model_status` and `cancel_generation` Tauri commands took `State<'_>` without returning `Result` (compile error).
- `count_footnotes` in `epub.rs` double-counted `<aside epub:type="footnote">` (regex matched both the tag and the attribute); now counts elements once, mirroring the parser's block classification.
- Clippy: unused `EpubLayout` import in `db.rs`, useless `format!` in `export.rs`, uninlined format args in `commands.rs`.
- Missing Tauri icons made `generate_context!` panic; generated PNG/ICO set from the repo SVG mark.

**Commands run (all passing):**
- `cargo test --workspace` (9 tests across 3 targets), `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`
- `npx tsc --noEmit`, `npm run build` (Next static export), `npm test` (vitest, 3 tests)
- `python3 scripts/validate_repo.py`, `python3 scripts/e2e_mock_worker.py`
- `PYTHONPATH=. uv run --with pytest --no-project pytest services/tts-worker/tests` (4 tests)

**Tests added:** `src/lib/import-selection.test.ts` — the repo claimed vitest coverage but shipped zero test files; `npm test` (and CI) failed on empty suite.

**Assumptions (could be wrong):**
- Reconstructed `src/lib/*` and the second half of `ReaderStudio.tsx` are my authorship inferred from usage, not the original code. Behavior verified only by typecheck/build, not by running the Tauri app end to end.
- `defaultImportSelection` defaults (footnotes skip, tables summary, captions read) inferred from `<select>` option ordering.
- Kokoro worker was validated with the mock engine only; real model download/generation untested.

**Not recovered (did not exist anywhere):** `docs/agent/*` convention files; the original vertest frontend tests; `tauri.conf.json` macOS `.icns` icon (Linux/Windows icons generated).

**Risk tier:** medium (reconstructed UI code is unexercised against a real EPUB in the running app).

**Rollback:** `git revert` the consolidation commits, or reset to tag `pre-consolidation-main` (2430b0e). Deleted branches are preserved as `archive/*` tags.

**Status:** done

## Follow-up session (2026-07-16): first real-run fixes and redesign

- **Reader "TypeError: Load failed"**: the CSP `connect-src` did not allow `asset:`/`http://asset.localhost`, so `fetch()` of the imported EPUB was blocked. Added the asset protocol to `connect-src` in `tauri.conf.json`. Needs an in-app retest with a real book.
- **Worker install failed on Python 3.14**: system Python is outside Kokoro's `>=3.10,<3.14` range. `ensure_worker_environment` now prefers `uv` (`uv venv --python 3.12` + `uv pip install`), which downloads a managed interpreter; the old `python3 -m venv` path remains as fallback. Added `services/tts-worker/.python-version` (3.12) and `AUDIOBOOKGEN_UV` override.
- **UI redesign**: replaced the dark acid-lime theme with an editorial paper aesthetic (warm paper ground, ink text, hairline rules, serif display via Iowan Old Style/Palatino/Georgia stack, letterspaced uppercase labels, monochrome buttons that invert on hover). All CSS class names kept; markup changes limited to the wordmark. Reader highlight color moved to a soft highlighter yellow.
- **Tests added (TDD)**: `epub.rs::counts_footnote_elements_once_each` (locks the double-count fix, covers aside/epub:type/doc-note/doc-endnote variants); `src/lib/tauri.test.ts` (isTauri/mediaUrl behavior outside Tauri). `scripts/validate_repo.py` now requires the reconstructed `src/lib` modules and the icon so a future incomplete checkout fails validation.
- Verified: clippy -D warnings, fmt, 10 Rust tests, tsc, Next build, 7 vitest tests, repo validator, 4 pytest, app boot on Hyprland.
