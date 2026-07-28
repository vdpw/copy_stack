# Clipboard Event Flows

## Event Names

Backend-to-frontend events:

- `clipboard-history-updated`: history changed; frontend should call
  `get_copy_events`.
- `app:navigate`: tray requested a view; payload is `history` or `settings`.
- `app-language-changed`: language preference or resolution changed; every
  frontend window should reload `get_app_settings`.

The current app does not use `new-copy-event`.

## Capture Flow

```mermaid
sequenceDiagram
  participant Clipboard as System Clipboard
  participant Listener as Listener Thread
  participant Backend as Tauri Backend
  participant DB as SQLite
  participant Tray as Tray Menu
  participant UI as React UI

  Clipboard->>Listener: clipboard content changes
  Listener->>Backend: send Event over mpsc channel
  Backend->>DB: classify data and compute content hash
  Backend->>Backend: check restore suppression
  Backend->>DB: insert new row or refresh duplicate payload
  Backend->>Tray: rebuild menu
  Backend->>UI: emit clipboard-history-updated
  UI->>Backend: invoke get_copy_events
  Backend->>DB: read ordered rows
  Backend->>UI: return StoredEvent[]
```

Important rules:

- The listener polls every 500 milliseconds.
- Events with no supported public clipboard representation are discarded before
  database insertion.
- Duplicate content updates the existing row payload and preserves its order.
- The UI reloads from SQLite instead of inserting optimistic rows.
- Tray sync runs after successful persistence.
- When the history JSONL mirror is enabled, the backend rewrites it after
  successful persistence before tray/UI refresh.

## Restore From Main Window

```mermaid
sequenceDiagram
  participant UI as React UI
  participant Backend as Tauri Backend
  participant DB as SQLite
  participant Clipboard as System Clipboard
  participant Tray as Tray Menu

  UI->>Backend: invoke copy_to_clipboard(content_hash)
  Backend->>DB: load stored event and settings
  Backend->>Backend: queue suppression if restore ordering is disabled
  Backend->>Clipboard: write stored Event
  alt move_restored_item_to_top is true
    Backend->>DB: move row to top
    Backend->>Tray: sync menu
    Backend->>UI: emit clipboard-history-updated
    UI->>Backend: invoke get_copy_events
  end
```

The frontend does not call `loadEvents()` after `copy_to_clipboard` resolves.
When ordering is unchanged there is nothing to reload. When ordering changes,
the backend notification triggers one background reload while the existing
list remains mounted, preserving thumbnails and the current scroll position.

## Restore From Tray Menu

The tray menu item id is `event::<content-hash>`. Selecting it runs
`restore_event(...)` in `src-tauri/src/tray.rs`.

Flow:

1. Load the stored event, content hash, and restore-order setting.
2. Queue suppression if restore ordering is disabled.
3. Write the event back to the clipboard.
4. If restore ordering is enabled, move the row to the top.
5. If ordering changed, notify the frontend and sync the tray.

When restore ordering is disabled, the code writes to the clipboard but does not
notify or sync immediately because the stored list did not change.

## Delete One Item

Frontend flow:

1. User clicks the delete button.
2. `delete_copy_event` is invoked with `{ contentHash }`.
3. Backend deletes the row.
4. Backend syncs the tray.
5. Frontend reloads history after the command returns.

Current backend behavior does not emit `clipboard-history-updated` for
`delete_copy_event`; the caller is expected to reload after the command.

## Clear All History

From the frontend:

1. User clicks Clear all.
2. `clear_all_events` deletes all rows.
3. Backend syncs the tray.
4. Frontend reloads history after the command returns.

From the tray:

1. User selects Clear history.
2. Backend deletes all rows.
3. Backend emits `clipboard-history-updated`.
4. Backend syncs the tray.

## Change Retention

Frontend flow:

1. User edits `max_items`.
2. UI validates 1 to 1000.
3. If the new limit is below the current visible count, UI asks for
   confirmation.
4. `set_max_items` stores the setting and trims old rows.
5. Backend syncs the tray.
6. Frontend reloads history after the command returns.

## Toggle Tray Visibility

Frontend flow:

1. User toggles tray visibility.
2. `set_show_in_menu_bar` stores `show_in_menu_bar`.
3. Backend syncs the tray.
4. `tray.set_visible(...)` applies the setting.

If the tray is hidden, users must reopen the main window through the Dock or the
platform shell to turn it back on.

## Toggle Restore Ordering

Frontend flow:

1. User toggles restore ordering.
2. `set_move_restored_item_to_top` stores the setting.
3. Future restore actions either move rows to the top or preserve current order.

This setting does not rewrite existing history.

## Toggle Compact Mode

Frontend flow:

1. User toggles compact mode.
2. `set_compact_mode` stores `compact_mode`.
3. Backend rebuilds the tray and emits `clipboard-history-updated`.
4. Frontend reloads history.

While enabled, new captures persist only recognizable, non-blank UTF-8 text.
Formatted text keeps its plain-text representation; image/file/video or
embedded-media events are filtered. Older full events remain stored but are
shown and restored as plain text only. Visible old rows with the same effective
text are deduplicated without changing the underlying rows until that text is
captured again in compact mode.

## Change Language

```mermaid
sequenceDiagram
  participant UI as Settings UI
  participant Backend as Tauri Backend
  participant DB as SQLite
  participant Native as App Menu and Window
  participant Tray as Tray Menu
  participant Windows as Open Webviews

  UI->>Backend: invoke set_language(language)
  Backend->>DB: persist language preference
  Backend->>Backend: resolve manual value or system locale
  Backend->>Native: rebuild app menu and settings title
  Backend->>Tray: rebuild localized tray menu
  Backend->>Windows: emit app-language-changed
  Windows->>Backend: invoke get_app_settings
  Backend->>Windows: return language and resolved_language
  Backend->>UI: return updated AppSettings
```

The command accepts `system`, `en`, `zh-CN`, or `zh-TW`. With `system`, the
backend uses `sys-locale` to choose a concrete supported language and falls back
to English when necessary. The initiating settings window applies the returned
`AppSettings`; the event keeps other open webviews synchronized. Native menu
and tray labels update without restarting the app. Stored clipboard content is
not translated.

## Payload Flow

Stored payloads are binary-encoded Rust values:

```text
copy_event_listener::event::Event -> binary event blob -> clipboard_events.event_data
```

For accepted events, the binary event blob preserves all data flavors reported
by the listener, including private or platform-specific metadata.
Classification requires a supported public flavor and selects it for
`content_hash`, `data_type`, and `display`; events containing only unsupported
flavors are not persisted. Compact mode additionally builds and persists a new
event containing only `public.utf8-plain-text`.

The backend returns stored `data_type` and binary `display` preview metadata for
history lists without sending raw `event_data` to React. It also returns an
optional `html_preview` for sandboxed expanded-card rendering and display-only
`rich_preview` segments when the stored event contains ordered mixed text/image
content. Restore operations use the backend to decode and pass the original
event back to `ClipboardListener::set_clipboard_event(...)`.

## Flow Change Checklist

- Decide whether the database order changes.
- If the order changes, update SQLite first and make the frontend reload from
  `get_copy_events`.
- If a tray action changes history, emit `clipboard-history-updated`.
- If a frontend command changes history but does not emit, reload explicitly
  after the command resolves.
- If language changes, rebuild native labels, emit `app-language-changed`, and
  reload `AppSettings` rather than copying a resolved locale between webviews.
- Keep restore suppression behavior intentional when writing to the clipboard.
