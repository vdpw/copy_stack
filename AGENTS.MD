# AGENTS.md

## Project Summary

`copy_stack` is a Tauri desktop clipboard manager with:

- a React 18 + TypeScript + Vite frontend in `src/`
- a Rust/Tauri backend in `src-tauri/`
- clipboard monitoring provided by the local path dependency `../copy_event_listener`
- SQLite persistence via `rusqlite`

The frontend talks to Rust through Tauri commands in `src-tauri/src/lib.rs` and receives live clipboard updates through the `"new-copy-event"` event channel.

## Repository Layout

- `src/App.tsx`: main UI, Tauri command calls, event subscription, clipboard payload decoding
- `src/App.css`: app styling
- `src-tauri/src/main.rs`: starts the clipboard listener thread and boots Tauri
- `src-tauri/src/lib.rs`: Tauri setup, shared app state, command handlers, event emission
- `src-tauri/src/store/database.rs`: SQLite schema, settings, deduplication, retention logic
- `src-tauri/tauri.conf.json`: Tauri app configuration

## Setup And Commands

- Install JS deps: `pnpm install`
- Web dev: `pnpm dev`
- Desktop dev: `pnpm desktop:dev`
- Web build: `pnpm build`
- Desktop build: `pnpm desktop:build`
- Type-check frontend: `pnpm type-check`
- Lint frontend: `pnpm lint`
- Format frontend docs/assets: `pnpm format`
- Check Rust backend: `cargo check --manifest-path src-tauri/Cargo.toml`
- Format Rust backend: `cargo fmt --manifest-path src-tauri/Cargo.toml`

## Working Notes

- The Rust backend currently stores data in `copy_stack.db` in the repo root. Treat it as local state, not source-controlled project data.
- Local development uses the path dependency `../copy_event_listener`, but GitHub Actions production builds are expected to switch that dependency to the published crate `copy_event_listener = "0.1.2"`.
- Clipboard events are stored as serialized `copy_event_listener::event::Event` JSON in SQLite, then decoded in the frontend for display.
- Duplicate clipboard content is deduplicated in `Database::insert_event` using a SHA-256 hash of the serialized event payload.
- The `max_items` setting is enforced in both Rust storage cleanup and frontend list rendering.
- The README still says clipboard monitoring is simulated, but the current code uses the real `copy_event_listener` path dependency. Prefer the code over the README when they conflict.

## Change Guidance

- If you add or rename a Tauri command, update both the Rust handler list in `src-tauri/src/lib.rs` and the corresponding frontend `invoke(...)` usage.
- If you change the event payload shape, keep Rust serialization and the TypeScript interfaces in `src/App.tsx` in sync.
- If you touch persistence or retention logic, verify behavior with an existing `copy_stack.db` instead of assuming a clean database.
- Avoid committing generated or machine-local files such as `copy_stack.db`, `dist/`, or `node_modules/`.

## Verification Expectations

For frontend-only changes, run `pnpm type-check` and `pnpm lint`.

For Rust-only or cross-stack changes, run:

- `cargo check --manifest-path src-tauri/Cargo.toml`
- `pnpm type-check`

For behavior changes involving clipboard capture, retention, or Tauri command wiring, also validate manually with `pnpm desktop:dev`.
