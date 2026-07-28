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
- System locale detection through `sys-locale`.

## Main Files

- `src-tauri/src/main.rs`: binary entry point.
- `src-tauri/src/lib.rs`: Tauri app setup, shared state, command handlers,
  listener event handling, restore suppression, and command registration.
- `src-tauri/src/i18n.rs`: supported languages and preferences, system locale
  resolution, and native menu/tray string catalogs.
- `src-tauri/src/store/database.rs`: database wrapper and all persistence logic.
- `src-tauri/src/tray.rs`: menu bar setup, tray menu sync, tray action handling,
  and frontend event emission.
- `src-tauri/src/store/mod.rs`: re-exports store types.
- `src-tauri/src/event/`: frontend payload structs and binary event blob
  encode/decode helpers.

## Application Startup

`main.rs` performs the native startup work:

1. Creates an `mpsc` channel.
2. Spawns a clipboard listener thread.
3. Configures the listener interval to 500 milliseconds.
4. Sends each captured `copy_event_listener::event::Event` through the channel.
5. Parses app-specific startup flags.
6. Calls `copy_stack_lib::run(rx, startup_options)`.

`lib.rs` then builds the Tauri app:

1. Installs the opener plugin.
2. Hides the main window instead of closing it.
3. Creates the database.
4. Stores `AppState` in Tauri managed state.
5. Resolves the persisted language preference and installs the localized native
   application menu.
6. Runs retention cleanup.
7. Writes the optional history JSONL mirror when enabled.
8. Sets up and syncs the localized tray menu.
9. Spawns a thread to consume clipboard events from `rx`.
10. Registers Tauri commands.

## Startup Flags

The binary accepts project-specific flags and ignores unrelated arguments:

- `--copy-stack-history-jsonl <path>` or
  `--copy-stack-history-jsonl=<path>` enables a JSONL mirror of
  `clipboard_events`.
- `--copy-stack-history-jsonl-max-data-bytes <bytes>` or
  `--copy-stack-history-jsonl-max-data-bytes=<bytes>` controls the maximum bytes
  written for each clipboard byte field. The default is `4096`.

When enabled, the app rewrites the JSONL file after startup cleanup and after
history mutations such as inserts, deletes, clears, retention trims, and
restore-to-top updates. The file can contain clipboard contents and should be
treated like the SQLite database.

## Shared State

```rust
pub struct AppState {
    pub(crate) db: Mutex<Database>,
    pub(crate) pending_restore_suppression: Mutex<Option<PendingRestoreSuppression>>,
    pub(crate) history_jsonl: Option<HistoryJsonlConfig>,
}
```

`Database` wraps a single `rusqlite::Connection`, so access is serialized behind
the mutex. Keep locks scoped tightly, especially before calling tray sync or
frontend event emission.

## Command Handlers

### `get_copy_events`

Returns stored event metadata ordered by `timestamp DESC, content_hash ASC`.
Rows include `content_hash`, backend-selected `data_type` and binary `display`,
optional `html_preview`, ordered `rich_preview` segments for mixed text/image
clips, and `timestamp`. They do not include raw `event_data`.

Standalone image file URLs also produce a `rich_preview` image segment by
reading the referenced local file. This allows file-originated image clips,
whose `display` value is only an extension label, to render a thumbnail.
When a stored event contains `public.html`, its decoded text is returned as
`html_preview` for the frontend's sandboxed expanded-card renderer.

Application-private clipboard formats are not decoded for previews, and the
backend does not inspect another application's cache directories. Private
flavors remain in a raw stored event when that event also contains a supported
public representation so restore operations can reproduce the original
clipboard payload. Events containing only private or otherwise unsupported
flavors are discarded before persistence.

### `delete_copy_event`

Deletes one row by content hash, then syncs the tray.

### `clear_all_events`

Deletes all history, then syncs the tray.

### `copy_to_clipboard`

Loads a stored event and writes it back to the system clipboard. When restore
ordering is enabled, it also moves the row to the top, syncs the tray, and
notifies the frontend. When ordering is disabled, it leaves the history and
tray untouched after the clipboard write.

When restore ordering is disabled, it queues restore suppression before writing
to the clipboard so the listener does not immediately reprocess that same
content.

### `get_event_by_content_hash`

Returns the decoded `copy_event_listener::event::Event` for a row. This is
registered as a command, although the current frontend does not call it.

### `get_app_settings`

Returns `max_items`, `show_in_menu_bar`, `move_restored_item_to_top`, and
`compact_mode`, plus:

- `language`: the persisted preference (`system`, `en`, `zh-CN`, or `zh-TW`).
- `resolved_language`: the concrete language currently selected after resolving
  `system` (`en`, `zh-CN`, or `zh-TW`).

### `set_max_items`

Stores the new limit, trims old events, syncs the tray, and notifies frontend
windows to reload history.

### `set_show_in_menu_bar`

Stores tray visibility and syncs the tray.

### `set_move_restored_item_to_top`

Stores restore ordering behavior.

### `set_compact_mode`

Stores compact-mode behavior, rebuilds the tray menu, and notifies frontend
windows to reload history. It does not destructively rewrite older full
events; those rows are projected to plain text while the setting is enabled.

### `set_language`

Validates and stores a `system`, `en`, `zh-CN`, or `zh-TW` preference. It then
resolves the concrete language, replaces the native application menu, updates
an existing settings-window title, rebuilds the tray menu, and emits
`app-language-changed` to all frontend windows. The command returns the complete
updated `AppSettings`, including both `language` and `resolved_language`.

Unsupported command values return an error without changing the setting.

## Language Resolution

The application supports English (`en`), Simplified Chinese (`zh-CN`), and
Traditional Chinese (`zh-TW`). A manual preference resolves directly to its
matching language. The default `system` preference asks `sys-locale` for the
operating system's ordered locale list and selects the first supported locale.

Locale matching is case-insensitive and accepts hyphenated or underscored tags.
`zh-Hant` and Chinese locales for Taiwan, Hong Kong, or Macao resolve to
`zh-TW`; other Chinese locales resolve to `zh-CN`. English variants resolve to
`en`. If none of the reported locales is supported, the backend falls back to
English.

The Rust catalog in `src-tauri/src/i18n.rs` owns native strings. The TypeScript
catalog in `src/i18n.ts` owns webview strings; keep their supported language
codes aligned.

## Clipboard Event Consumption

The background consumer thread in `lib.rs` receives events from the channel.
For each event:

1. When compact mode is enabled, extract a valid standalone plain-text event
   and filter events containing image, file, video, or embedded-media data.
2. Classify it into `content_hash`, `data_type`, and `display`.
3. Compare it with pending restore suppression.
4. Skip the event if it is the one app-initiated restore that should preserve
   order.
5. Insert or update the event through `Database::insert_event`.
6. Rewrite the optional history JSONL mirror when enabled.
7. Sync the tray menu.
8. Emit `clipboard-history-updated` so the frontend reloads from SQLite.

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
- Rebuilds the menu from database history and the resolved language during
  `sync`.
- Applies the `show_in_menu_bar` setting through `tray.set_visible(...)`.
- Emits `app:navigate` when the user selects History or Settings.
- Clears history from the menu.
- Restores a selected event from the menu.
- Emits `clipboard-history-updated` when the frontend must reload.
- Emits `app-language-changed` after a language update so open webviews reload
  `AppSettings`.

Tray menu item ids use stable prefixes:

- `event::<content-hash>` for clipboard items.
- `action::open-history`
- `action::open-settings`
- `action::clear-history`
- `action::quit`

## Tray Labels

Structural tray labels such as Recent clipboard items, Open history, Settings,
Clear history, the empty state, and Quit are selected from the native catalog.
Language changes rebuild the menu immediately. Clipboard content labels remain
the user's stored content rather than translated text.

Clipboard labels decode the stored `display` bytes from the database classifier.
The top-level menu label truncates long results to 40 display-width characters,
counting CJK/full-width characters as 2 columns and ASCII characters as 1.
Overflow uses `...`. If truncation is needed, the event is rendered as a
submenu whose child item shows the full label and restores the clip when
selected. Plain text displays are normalized as one label. File and folder
displays parse the `copy_stack.file-items.v1` JSON payload and prefix each item
name with a file or folder marker. File item names come from the raw
`public.utf8-plain-text` filename list split on carriage returns, with generic
file/folder labels generated in the resolved language when no safe name is
available. Opaque reference ids such as `id=...` are never used as display
names. This keeps the tray and React history previews aligned while allowing
binary thumbnails to be stored in the same column later.

On macOS, `lib.rs` also builds localized application menu headings and
predefined actions. Startup replaces the initial system-localized menu with the
persisted preference after the database is available, and `set_language`
replaces it again immediately.

## Backend Change Checklist

- Register new commands in `tauri::generate_handler!`.
- Update frontend `invoke(...)` calls for command changes.
- Keep emitted event names synchronized with frontend listeners.
- Keep the Rust native catalog and TypeScript catalog language codes
  synchronized.
- Avoid holding `state.db` locks while doing unrelated work.
- Run `cargo fmt --manifest-path src-tauri/Cargo.toml`.
- Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- For cross-stack changes, also run `pnpm type-check`.
