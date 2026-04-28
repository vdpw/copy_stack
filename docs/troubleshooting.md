# Troubleshooting

## `pnpm desktop:dev` Cannot Start Vite

Vite uses port `5173` with `strictPort: true`. If that port is already in use,
stop the existing process before running Tauri again.

Check:

```bash
lsof -i :5173
```

## Rust Cannot Find `copy_event_listener`

Local development expects this relative path from `src-tauri/Cargo.toml`:

```text
../../copy_event_listener
```

If `cargo check --manifest-path src-tauri/Cargo.toml` fails because the path
dependency is missing, check out the listener project in the expected location
or temporarily point the dependency to the published crate only for a targeted
release/build test.

## Clipboard Items Do Not Appear

Check these areas:

- The app was launched with `pnpm desktop:dev`, not only `pnpm dev`.
- The clipboard listener thread started in the Rust process.
- The listener is sending events over the `mpsc` channel.
- `Database::insert_event` is not returning an error.
- The backend emits `clipboard-history-updated`.
- The frontend listener calls `loadEvents()`.

In debug builds, the backend prints `[copy_stack]` logs around startup,
listener capture, persistence, restore, and tray refresh.

## Duplicate Items Behave Unexpectedly

Deduplication uses normalized content fragments, not the full serialized event.
For text-like clipboard items, whitespace is normalized and null characters are
removed before hashing.

If two payloads appear identical to the user but do not dedupe, inspect their
data types and decoded fragments. If two different payloads dedupe
unexpectedly, check whether the preferred text fragment is too coarse for that
clipboard type.

See `docs/persistence.md` and `docs/design/copy-event-ordering.md`.

## Restored Items Reappear Or Move Unexpectedly

Check `move_restored_item_to_top` in settings:

```bash
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT key, value FROM settings WHERE key = 'move_restored_item_to_top';"
```

When the setting is false, restore suppression should skip the listener echo
for the matching content hash within five seconds.

When the setting is true, restore actions intentionally update `sort_order` and
`timestamp`.

## Tray Menu Is Stale

Tray contents are rebuilt from SQLite during `tray::sync(...)`. Confirm the
history-changing action calls sync after the database mutation.

Actions that should sync:

- Listener event insert/update.
- `delete_copy_event`.
- `clear_all_events`.
- `copy_to_clipboard`.
- `set_max_items`.
- `set_show_in_menu_bar`.
- Tray clear-history action.
- Tray restore action when restore ordering is enabled.

## Tray Menu Is Hidden

Check `show_in_menu_bar`:

```bash
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT key, value FROM settings WHERE key = 'show_in_menu_bar';"
```

If it is `false`, reopen the main window through the Dock or platform shell and
turn the setting back on.

## History Limit Does Not Match Visible Rows

Check:

- `max_items` in the `settings` table.
- Whether `cleanup_old_events()` ran after changing the setting.
- Whether the frontend reloaded history after `set_max_items`.
- Whether the tray was synced after cleanup.

Inspect:

```bash
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT COUNT(*) FROM clipboard_events;"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT key, value FROM settings WHERE key = 'max_items';"
```

## Frontend Preview Shows `Error parsing content`

The frontend could not parse `event_data` into the expected clipboard event
shape. Check whether the stored payload was written by an older incompatible
version or whether the upstream `copy_event_listener` event shape changed.

If the shape changed, update the TypeScript interfaces and preview decoding in
`src/App.tsx`.

## `pnpm lint` Fails On Unused Values

The ESLint config treats unused variables as errors, except arguments starting
with `_`. Remove unused values or intentionally prefix unused callback
arguments with `_`.

## `cargo check` Fails After Release Workflow Changes

The release workflow rewrites `src-tauri/Cargo.toml` in CI to use the published
clipboard listener crate. Local development should normally keep the path
dependency. If the local file was changed during release testing, restore the
path dependency before continuing development.

## Database Contains Sensitive Data

Clipboard history can include secrets, personal text, file URLs, and other
sensitive payloads. Do not attach raw `copy_stack.db` files to issues or commit
them. Reproduce issues with sanitized text whenever possible.
