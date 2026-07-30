# Architecture

## Component Map

```mermaid
flowchart TD
  OS["macOS NSPasteboard"] --> Listener["copy_event_listener thread"]
  Listener --> Channel["mpsc channel"]
  Channel --> Policy["protocol + resource policy"]
  Policy --> DB["SQLite private store"]
  DB --> Summaries["cursor summaries / configurable tray snapshot"]
  DB --> Seeds["owned detail / restore seeds"]
  Seeds --> Detail["detail decode outside DB lock"]
  DB --> Mirror["coalescing JSONL worker<br/>independent read connection"]
  Summaries --> Tray["native menu bar"]
  Summaries --> UI["React History"]
  Detail --> UI
  UI --> Commands["capability-scoped Tauri commands"]
  Commands --> DB
  Commands --> OS
```

## Runtime Lifecycle

1. `main.rs` parses project startup flags and calls `run(startup_options)`.
2. The Tauri builder registers `tauri-plugin-single-instance` before every
   other plugin. A later process asks the first process to show, unminimize, and
   focus its main window, then exits before app setup.
3. The autostart plugin is registered with the internal
   `--copy-stack-autostart` argument. Autostart remains off until the user
   enables the operating system login item.
4. First-instance setup applies the launch visibility policy. Manual launches
   show the main window; login launches keep it hidden.
5. Setup opens the private SQLite database, applies only required transactional
   schema/classifier migrations, and runs count/byte retention.
6. If requested by startup flags, setup starts the JSONL worker and schedules a
   row-free refresh after cleanup; the worker reads committed rows on its own
   connection.
7. Setup creates managed state, localized native menus, the menu bar, and the
   clipboard listener and storage threads.
8. On application exit, the JSONL worker is asked to flush its latest
   generation and stop within two seconds. A timeout is recorded as a redacted
   diagnostic rather than blocking exit indefinitely.

A current `PRAGMA user_version` and classifier metadata version take the fast
path: startup validates schema/index shape but does not decode, reclassify, or
rewrite every history row.

## Backend State

`AppState` contains:

- `db: Mutex<Database>`: one `rusqlite::Connection`;
- `pending_restore_suppression`: one short-lived content identity used to
  ignore the listener echo of an app restore;
- `history_mirror: Option<HistoryMirror>`: the background JSONL scheduler and
  worker;
- `diagnostics: DiagnosticLog`: at most 32 redacted diagnostic records.

Database access remains serialized, but expensive work is separated from the
lock:

- list and menu-construction queries select only persisted summary columns;
- macOS tray hover reads one bounded display value for the currently
  highlighted text row and presents it in a native nonactivating panel;
- detail and restore commands copy an owned seed under the lock, then decode
  event data and inspect media after releasing it;
- JSONL refresh signals are sent after a committed mutation. One coalescing
  worker reads the latest rows through an independent read-only connection,
  then decodes, serializes, flushes, syncs, and atomically renames them.

## Frontend Structure

- `src/App.tsx`: owns the main window's History/Settings page navigation from
  native `app:navigate` requests and the Settings back button, and coordinates
  language/error presentation.
- `src/features/history/`: history list, cards, media presentation, and the
  bounded detail cache.
- `src/features/settings/`: settings presentation.
- `src/hooks/useClipboardHistory.ts`: 50-item cursor paging and refresh
  generation control.
- `src/hooks/useHistoryDetails.ts`: on-expansion detail loading.
- `src/hooks/useAppSettings.ts`: authoritative settings/autostart reads and
  optimistic mutation rollback.
- `src/api/tauri.ts`: typed command invocation and safe error normalization.
- `src/lib/htmlPreview.ts`: allowlist sanitizer and isolated preview document.
- `src/types.ts`: the TypeScript side of serialized Rust contracts.

The History page keeps scroll and expansion state across live refreshes. The
Settings page loads counts and byte totals from `get_app_settings`; because the
History view is unmounted while Settings is active, it does not fetch clipboard
history in the background. The macOS application menu and `Command+,` are the
primary Settings entry points; the tray entry remains available. Settings
returns to History through its own back button.

## Command Contract

The single main window registers the history commands
`get_copy_events_page`, `get_history_detail`, `delete_copy_event`,
`clear_all_events`, and `copy_to_clipboard`; the settings commands
`get_app_settings`, `get_autostart_status`, `set_autostart_enabled`,
`set_max_items`, `set_max_history_bytes`, `set_show_in_menu_bar`,
`set_menu_bar_item_limit`, `set_move_restored_item_to_top`,
`set_compact_mode`, and `set_language`; plus the startup and diagnostic reads.

`src-tauri/capabilities/main.json` grants exactly that audited command union to
the one webview. There is no broad `core:default` grant, separate settings
window capability, or opener plugin.

## Event Contract

- `clipboard-history-updated`: reload the first history page while preserving
  view state.
- `app:navigate`: show and focus the main window, then select `history` or
  `settings` when a native menu requests it.
- `app-language-changed`: reload settings so both pages use the backend's
  resolved language.
- `capture-rejected`: show a localized notice for a resource-limit rejection;
  the payload contains only a resource code and size bucket.

The app does not use `new-copy-event`.

## Ordering And Paging

History is ordered by:

```sql
ORDER BY timestamp DESC, content_hash ASC
```

The opaque `v1` cursor contains the last row's timestamp and lowercase content
hash. Subsequent pages use the same tuple comparison, avoiding offset drift and
handling equal timestamps deterministically. The default page size is 50 and
the backend clamps every request to 100. The current schema has no `sort_order`
column; legacy `sort_order` is translated during migration only.

## Security Boundaries

Production CSP denies external connections and unsafe script/style execution
and disables objects, forms, base URLs, and embedding. The unused Tauri asset
protocol is disabled. Bounded local image bytes become revocable in-memory
`blob:` URLs, while video details expose only a display label and media type,
never a full local path.

HTML previews are input-, node-, and depth-bounded, rebuilt from an element and
attribute allowlist, stripped of every resource URL, placed in a sandboxed
iframe, and receive inner `default-src 'none'; img-src 'none'` policy. The outer
production policy also disallows `data:` images. Command errors expose only
stable code, operation, and retryability. Native/manual verification
requirements remain tracked in `docs/security-release-checklist.md`.
