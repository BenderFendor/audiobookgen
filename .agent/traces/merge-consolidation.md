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
