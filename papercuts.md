## 2026-07-16 00:00

**What happened:** audiobookgen: post-tool lint hook wants tsgolint but oxlint-tsgolint is not installed; every JS/TS edit reports a spurious lint failure

---

## 2026-07-16 01:06

**What happened:** Global AGENTS router referenced missing scripts/agent-summary and docs/agent/known-errors.md in audiobookgen; fallback exploration was required

---

## 2026-07-16 01:08

**What happened:** Installing gst-plugins-good alone hit an exact-version GStreamer dependency conflict after package databases advanced from 1.28.3 to 1.28.4; the full package set must be upgraded together

---

## 2026-07-16 01:27

**What happened:** Post-edit lint hook expects a missing tsgolint executable, obscuring otherwise useful TypeScript verification output

---

## 2026-07-16 01:28

**What happened:** Documented hyprctl dispatch workspace syntax failed under the installed Lua-dispatch Hyprland CLI, blocking the first live-window screenshot attempt

---

## 2026-07-16 01:38

**What happened:** A shell quoting mistake in a combined hyprctl and inspector curl command caused an avoidable diagnostic miss

---

## 2026-07-16 01:51

**What happened:** The first watchdog invocation of worker unit tests omitted the documented PYTHONPATH, producing a false dependency failure before the corrected rerun

---

## 2026-07-16 01:52

**What happened:** A jq precedence mistake failed while summarizing otherwise completed watchdog reports; parenthesizing the command join fixed it

---

## 2026-07-16 07:30

**What happened:** oxlint post-tool hook fails on this repo: tsgolint executable missing (oxlint-tsgolint not installed); TS type errors still surface via tsc but JS lint step always errors

---

## 2026-07-16 08:18

**What happened:** Post-edit lint hook requires undeclared oxlint-tsgolint even though the repo uses tsc and Vitest; hook failure obscures otherwise valid TypeScript edits.

---

## 2026-07-16 08:35

**What happened:** PreToolUse sensitive-path hook blocked adding a model vocabulary source file because ordinary inference identifiers were mistaken for secrets.

---

## 2026-07-16 08:46

**What happened:** Verification command drift: worker pyproject has no test extra and Vitest rejects Jest's --runInBand flag; use the repository's actual dependency group and plain npm test.

---

## 2026-07-16 09:02

**What happened:** GPU watchdog command failed immediately because the app's production worker environment intentionally omits pytest; hardware tests should use stdlib unittest or the development environment.

---

## 2026-07-16 09:09

**What happened:** Frontend visual verifier could not attach because Chrome is not running and no DevToolsActivePort exists; actual Tauri runtime smoke continued, but Chrome MCP visual checks were skipped per repo rule.

---

## 2026-07-16 09:12

**What happened:** A metadata-inspection find pipeline produced expected SIGPIPE noise after head exited; prefer a bounded rg/read command for Hugging Face metadata.

---

