# AGENTS.md

This file is the compact project menu for coding agents. Keep detailed
explanations in `docs/` and load them on demand.

## What This Project Is

`copy_stack` is a Tauri 2 desktop clipboard manager. The UI is React 18,
TypeScript, and Vite in `src/`; the backend is Rust in `src-tauri/`; clipboard
capture comes from the local path dependency `../../copy_event_listener`; local
history and settings are stored in SQLite via `rusqlite`.

The app captures clipboard events, stores normalized history in SQLite, renders
history in the Tauri window, and mirrors recent items into the macOS menu bar.

## Runtime And Tooling

- Package manager: `pnpm`.
- Frontend dev server: Vite on port `5173`.
- Desktop shell: Tauri 2.
- Rust edition: 2021.
- Database: `$HOME/.copy_stack/copy_stack.db`.
- Local clipboard dependency: `copy_event_listener = { path = "../../copy_event_listener" }`.
- Release workflow swaps that local dependency to `copy_event_listener = "0.1.2"`.

## Commands

- Install JS deps: `pnpm install`
- Web dev only: `pnpm dev`
- Desktop dev: `pnpm desktop:dev`
- Web build: `pnpm build`
- Desktop build: `pnpm desktop:build`
- Frontend type-check: `pnpm type-check`
- Frontend lint: `pnpm lint`
- Frontend/doc format: `pnpm format`
- Rust check: `cargo check --manifest-path src-tauri/Cargo.toml`
- Rust format: `cargo fmt --manifest-path src-tauri/Cargo.toml`

## Documentation Menu

Start with `docs/index.md`, then load only the docs relevant to the task.

- Product and scope: `docs/project-overview.md`
- System architecture: `docs/architecture.md`
- Frontend guide: `docs/frontend.md`
- Backend guide: `docs/backend.md`
- Persistence and data model: `docs/persistence.md`
- Clipboard event flows: `docs/clipboard-flows.md`
- Development workflow: `docs/development.md`
- Release workflow: `docs/release.md`
- Troubleshooting: `docs/troubleshooting.md`
- Ordering and dedupe design decision: `docs/design/copy-event-ordering.md`

## Read Before Modifying

- UI, state, payload decoding, or Tauri `invoke(...)` calls: read `docs/frontend.md`.
- Tauri commands, listener wiring, restore behavior, or tray sync: read `docs/backend.md`.
- SQLite schema, migrations, retention, or deduplication: read `docs/persistence.md`.
- Clipboard capture, restore, menu item selection, or live refresh: read `docs/clipboard-flows.md`.
- Builds, checks, local dependency setup, or manual QA: read `docs/development.md`.
- GitHub release behavior or packaged builds: read `docs/release.md`.

## Non-Obvious Constraints

- Clipboard monitoring is real, not simulated. The current code uses the
  `copy_event_listener` crate.
- The frontend listens for `clipboard-history-updated` and `app:navigate`, not
  `new-copy-event`.
- History order is persisted in SQLite with `sort_order`; do not reorder only in
  React.
- Deduplication uses a normalized content hash, not a hash of the full serialized
  event payload.
- The app preserves restored-item order by default. It queues a short restore
  suppression so the listener does not reinsert the app's own clipboard write.
- Keep TypeScript payload interfaces in `src/App.tsx` synchronized with Rust
  serialization in `src-tauri/src/store/database.rs` and Tauri commands in
  `src-tauri/src/lib.rs`.
- Avoid committing generated or local files such as `node_modules/`, `dist/`,
  `src-tauri/target/`, `src-tauri/gen/`, and SQLite database files.

## Verification Expectations

- Frontend-only changes: run `pnpm type-check` and `pnpm lint`.
- Rust-only changes: run `cargo check --manifest-path src-tauri/Cargo.toml`.
- Cross-stack changes: run `cargo check --manifest-path src-tauri/Cargo.toml`
  and `pnpm type-check`.
- Clipboard capture, retention, restore-order, menu bar, or command wiring
  changes also need manual validation with `pnpm desktop:dev`.
