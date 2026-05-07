# Backend Guide

## Stack

- Rust 2021.
- Tauri 2 with tray icon support.
- `tauri-plugin-opener`.
- SQLite through `rusqlite` with the bundled SQLite feature.
- JSON API payloads with `serde`; persisted clipboard events use a compact
  binary blob format.
- Timestamps with `chrono`.
- Content hashing with `sha2`.
- Clipboard capture and restore through `copy_event_listener`.

## Main Files

- `src-tauri/src/main.rs`: binary entry point.
- `src-tauri/src/lib.rs`: Tauri app setup, shared state, command handlers,
  listener event handling, restore suppression, and command registration.
- `src-tauri/src/store/database.rs`: database wrapper and all persistence logic.
- `src-tauri/src/tray.rs`: menu bar setup, tray menu sync, tray action handling,
  and frontend event emission.
- `src-tauri/src/store/mod.rs`: re-exports store types.
- `src-tauri/src/event/`: frontend payload structs, clipboard event filtering,
  and binary event blob encode/decode helpers.

## Application Startup

`main.rs` performs the native startup work:

1. Creates an `mpsc` channel.
2. Spawns a clipboard listener thread.
3. Configures the listener interval to 500 milliseconds.
4. Sends each captured `copy_event_listener::event::Event` through the channel.
5. Calls `copy_stack_lib::run(rx)`.

`lib.rs` then builds the Tauri app:

1. Installs the opener plugin.
2. Hides the main window instead of closing it.
3. Creates the database.
4. Stores `AppState` in Tauri managed state.
5. Runs retention cleanup.
6. Sets up and syncs the tray menu.
7. Spawns a thread to consume clipboard events from `rx`.
8. Registers Tauri commands.

## Shared State

```rust
pub struct AppState {
    pub(crate) db: Mutex<Database>,
    pub(crate) pending_restore_suppression: Mutex<Option<PendingRestoreSuppression>>,
}
```

`Database` wraps a single `rusqlite::Connection`, so access is serialized behind
the mutex. Keep locks scoped tightly, especially before calling tray sync or
frontend event emission.

## Command Handlers

### `get_copy_events`

Returns stored event metadata ordered by `timestamp DESC, content_hash ASC`.
Rows include `content_hash`, backend-selected `data_type` and binary `display`,
and `timestamp`; they do not include decoded `event_data`.

### `delete_copy_event`

Deletes one row by content hash, then syncs the tray.

### `clear_all_events`

Deletes all history, then syncs the tray.

### `copy_to_clipboard`

Loads a stored event, writes it back to the system clipboard, optionally moves
the row to the top, syncs the tray, and notifies the frontend.

When restore ordering is disabled, it queues restore suppression before writing
to the clipboard so the listener does not immediately reprocess that same
content.

### `get_event_by_content_hash`

Returns the decoded `copy_event_listener::event::Event` for a row. This is
registered as a command, although the current frontend does not call it.

### `get_app_settings`

Returns `max_items`, `show_in_menu_bar`, and `move_restored_item_to_top`.

### `set_max_items`

Stores the new limit, trims old events, syncs the tray, and notifies frontend
windows to reload history.

### `set_show_in_menu_bar`

Stores tray visibility and syncs the tray.

### `set_move_restored_item_to_top`

Stores restore ordering behavior.

## Clipboard Event Consumption

The background consumer thread in `lib.rs` receives events from the channel.
For each event:

1. Classify it into `content_hash`, `data_type`, and `display`.
2. Compare it with pending restore suppression.
3. Skip the event if it is the one app-initiated restore that should preserve
   order.
4. Insert or update the event through `Database::insert_event`.
5. Sync the tray menu.
6. Emit `clipboard-history-updated` so the frontend reloads from SQLite.

## Restore Suppression

`RESTORE_SUPPRESSION_TTL` is five seconds. Suppression is used only when
`move_restored_item_to_top` is false. It stores the content hash of the event
being restored and consumes that suppression on the first matching listener
event.

If writing to the clipboard fails, the pending suppression is cleared when the
hash matches.

## Tray Menu

`src-tauri/src/tray.rs` owns all tray behavior:

- Creates the tray icon with id `main`.
- Rebuilds the menu from database history during `sync`.
- Applies the `show_in_menu_bar` setting through `tray.set_visible(...)`.
- Emits `app:navigate` when the user selects History or Settings.
- Clears history from the menu.
- Restores a selected event from the menu.
- Emits `clipboard-history-updated` when the frontend must reload.

Tray menu item ids use stable prefixes:

- `event::<content-hash>` for clipboard items.
- `action::open-history`
- `action::open-settings`
- `action::clear-history`
- `action::quit`

## Tray Labels

Tray labels decode the stored `display` bytes from the database classifier and
truncate the result to 72 characters. Plain text displays are normalized as one
label. File and folder displays parse the `copy_stack.file-items.v1` JSON
payload and prefix each item name with a file or folder marker. File item names
come from the raw `public.utf8-plain-text` filename list split on carriage
returns, with generic `File N` / `Folder N` fallbacks. Finder reference ids such
as `id=...` are never used as display names. This keeps the tray and React
history previews aligned while allowing binary thumbnails to be stored in the
same column later.

## Backend Change Checklist

- Register new commands in `tauri::generate_handler!`.
- Update frontend `invoke(...)` calls for command changes.
- Keep emitted event names synchronized with frontend listeners.
- Avoid holding `state.db` locks while doing unrelated work.
- Run `cargo fmt --manifest-path src-tauri/Cargo.toml`.
- Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- For cross-stack changes, also run `pnpm type-check`.
