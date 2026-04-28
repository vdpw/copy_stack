# Copy Stack Documentation Index

This directory holds the detailed project documentation. The root `AGENTS.md`
is intentionally a compact menu; use this index to choose the right deeper
document for the task at hand.

## Start Here

- `docs/project-overview.md`: product scope, user workflows, supported behavior,
  and current limitations.
- `docs/architecture.md`: component map, runtime lifecycle, threads, shared
  state, command boundary, and event boundary.
- `docs/development.md`: setup, commands, verification matrix, local database
  handling, and manual QA checklist.

## Implementation Areas

- `docs/frontend.md`: React state model, Tauri command usage, event listeners,
  clipboard payload decoding, and styling conventions.
- `docs/backend.md`: Rust entry point, Tauri setup, command handlers, tray menu,
  restore suppression, and module responsibilities.
- `docs/persistence.md`: SQLite location, schema, settings, migrations,
  persisted ordering, normalized content hashing, deduplication, and retention.
- `docs/clipboard-flows.md`: capture, restore, tray selection, clear/delete, and
  settings update flows.

## Operations

- `docs/release.md`: tag-based GitHub release workflow, macOS targets, local
  dependency replacement, and release checklist.
- `docs/troubleshooting.md`: common development, runtime, database, tray, and
  build issues.

## Design Records

- `docs/design/copy-event-ordering.md`: design decision for persisted ordering,
  stable content hashes, deduplication, and UI refresh behavior.

## Reading Paths

For a UI change, read:

1. `docs/frontend.md`
2. `docs/clipboard-flows.md` if the change touches live updates or restore
3. `docs/development.md` for the verification commands

For a Rust command or tray change, read:

1. `docs/backend.md`
2. `docs/clipboard-flows.md`
3. `docs/persistence.md` if the change touches stored history or settings

For a persistence change, read:

1. `docs/persistence.md`
2. `docs/design/copy-event-ordering.md`
3. `docs/clipboard-flows.md`

For a release or packaging change, read:

1. `docs/release.md`
2. `docs/development.md`
3. `src-tauri/tauri.conf.json`
4. `.github/workflows/release.yml`

## Documentation Rules

- Prefer code over stale prose when a conflict is found.
- Update the relevant detailed doc whenever behavior changes.
- Keep `AGENTS.md` short and menu-like. Put architecture, workflow, and data
  details here in `docs/`.
- Keep command names, event names, setting keys, and database paths exact.
