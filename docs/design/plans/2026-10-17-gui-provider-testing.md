# GUI Provider Testing Implementation Plan

> Execution: native, as requested; no approval pauses or tests in this phase.

**Goal:** Connect provider accounts/models in Settings and prepare observable Mac desktop runs.
**Architecture:** Provider-owned authentication processes; discovery in kai-core; asynchronous state in kai-app; execution through existing Kai policy boundaries.
**Tech Stack:** Rust, Tokio, reqwest, GPUI, JSON-RPC over stdio.
**Spec:** docs/superpowers/specs/2026-10-17-gui-provider-testing-design.md

## Global constraints
No tests, inference or live computer runs. No subscription credential persistence. Preserve prior work. No silent model substitution.

## Review focus
Missing binaries; failed/canceled login; paginated or stale catalogs; text-only models; shell requests in GUI-only mode.

## Tasks
- [x] Core discovery, Codex account RPC lifecycle, binary resolution, bounded errors/timeouts.
- [x] Claude Code tool-disabled decision bridge and provider-owned authentication controls.
- [x] Settings, Models and quick selector using provider catalogs and exact IDs.
- [x] GUI testing preset, shell guard, safe task draft and monitoring guidance.
- [x] Architecture/evidence review and deferred GUI acceptance checklist.
- [x] Compile/package without tests; inspect changes; commit source and push; fast-forward main only when safe.

## Completion evidence

Workspace compilation and release packaging completed without executing tests or model inference. Source review reported no remaining important findings in the provider/GUI-guard pathway. The package inventory excludes raw local runs and provider auth files; the ZIP checksum and installed app signature were verified. Testing remains the separate unchecked acceptance checklist in docs/MAC_GUI_TESTING.md.
