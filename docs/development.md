# Development Workflow

## Prerequisites

- Node.js 18 or newer.
- `pnpm`.
- Rust stable.
- Platform dependencies required by Tauri.
- The local `copy_event_listener` project checked out at
  `../../copy_event_listener` relative to `src-tauri/Cargo.toml`.

The local dependency path is required for normal development:

```toml
copy_event_listener = { path = "../../copy_event_listener" }
```

The release workflow replaces it with the published crate before packaging.

## Install

```bash
pnpm install
```

## Run

Web-only development:

```bash
pnpm dev
```

Desktop development:

```bash
pnpm desktop:dev
```

`desktop:dev` runs Tauri, which starts Vite through the Tauri config. Vite uses
port `5173` with `strictPort: true`.

## Build

Web build:

```bash
pnpm build
```

Desktop build:

```bash
pnpm desktop:build
```

## Checks

Frontend type-check:

```bash
pnpm type-check
```

Frontend lint:

```bash
pnpm lint
```

Rust check:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Rust format:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
```

Frontend and docs format:

```bash
pnpm format
```

## Verification Matrix

| Change type                              | Required checks                                                                                  |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------ |
| Frontend-only UI or TypeScript           | `pnpm type-check`, `pnpm lint`                                                                   |
| Rust-only backend                        | `cargo check --manifest-path src-tauri/Cargo.toml`                                               |
| Tauri command contract                   | `cargo check --manifest-path src-tauri/Cargo.toml`, `pnpm type-check`, manual `pnpm desktop:dev` |
| Clipboard capture or restore             | `cargo check --manifest-path src-tauri/Cargo.toml`, `pnpm type-check`, manual `pnpm desktop:dev` |
| Persistence, ordering, dedupe, retention | `cargo check --manifest-path src-tauri/Cargo.toml`, manual test with an existing database        |
| Release workflow                         | `pnpm type-check`, `pnpm lint`, `pnpm desktop:build` when local signing/platform setup allows    |
| Documentation-only                       | Review links and paths; no build is normally required                                            |

## Manual QA Checklist

Use `pnpm desktop:dev` for behavior changes.

1. Launch the app.
2. Copy new text from outside the app.
3. Confirm the item appears in History and in the tray menu.
4. Copy the same text again and confirm it does not create a duplicate.
5. Restore an item from the main window.
6. Toggle restore ordering and confirm restore behavior matches the setting.
7. Restore an item from the tray menu.
8. Delete one item and confirm the tray updates.
9. Clear all history from the app and from the tray.
10. Reduce `max_items` below the current count and confirm old items are
    trimmed after confirmation.
11. Toggle menu bar visibility.
12. Close the main window and confirm the app keeps running.

## Local Database Handling

The database lives at:

```text
$HOME/.copy_stack/copy_stack.db
```

Use an existing database when testing migrations, deduplication, ordering, or
retention. Do not rely only on a clean database.

Useful inspection commands:

```bash
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT key, value FROM settings ORDER BY key;"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT substr(content_hash, 1, 12), data_type, source_app, hex(substr(display, 1, 24)), timestamp FROM clipboard_events ORDER BY timestamp DESC LIMIT 10;"
```

Clipboard history can contain sensitive data. Do not commit copied databases or
paste payloads into issues, logs, docs, or test fixtures unless they are
sanitized.

## Generated And Local Files

Do not commit:

- `node_modules/`
- `dist/`
- `dist-ssr/`
- `src-tauri/target/`
- `src-tauri/gen/`
- SQLite database files
- local logs

## Common Change Patterns

### Add A Tauri Command

1. Add the command function in `src-tauri/src/lib.rs`.
2. Add it to `tauri::generate_handler!`.
3. Call it from the frontend with `invoke(...)`.
4. Update `docs/backend.md` and `docs/frontend.md`.
5. Run Rust and frontend checks.

### Change Event Payload Shape

1. Update Rust serialization or upstream event handling.
2. Update TypeScript interfaces in `src/App.tsx`.
3. Update preview decoding if needed.
4. Test with stored rows in an existing database.
5. Update `docs/frontend.md`, `docs/persistence.md`, and
   `docs/clipboard-flows.md`.

### Change Ordering Or Deduplication

1. Read `docs/design/copy-event-ordering.md`.
2. Update `src-tauri/src/store/database.rs`.
3. Verify migration behavior with an existing database.
4. Confirm UI and tray order match SQLite order.
5. Update persistence and flow docs.

### Change Tray Behavior

1. Update `src-tauri/src/tray.rs`.
2. Check whether the action mutates history.
3. Emit `clipboard-history-updated` if another frontend reload path is needed.
4. Sync the tray after history or visibility changes.
5. Validate manually with `pnpm desktop:dev`.
