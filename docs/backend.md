# Backend Guide

## Stack And Modules

- Rust 2021 and Tauri 2.
- `tauri-plugin-single-instance` for first-process ownership and activation.
- `tauri-plugin-autostart` for the operating system login item.
- SQLite through bundled `rusqlite`.
- Clipboard capture/restore through published `copy_event_listener = "0.1.2"`.
- `serde`, `chrono`, `sha2`, and `sys-locale`.

Important modules:

- `main.rs`: parse startup arguments and enter the library runtime.
- `lib.rs`: Tauri setup, command handlers, capture pipeline, shared state, and
  exit handling.
- `lifecycle.rs`: testable initial visibility, duplicate-launch activation, and
  verified autostart policies.
- `pasteboard_protocol.rs`: event-wide NSPasteboard marker assessment and
  canonical restore.
- `resource_policy.rs`: capture, preview, IPC, and history byte budgets.
- `command_error.rs`: structured errors and bounded redacted diagnostics.
- `private_fs.rs`: Unix ownership/type/link checks and `0700`/`0600` storage.
- `history_mirror.rs`: coalescing asynchronous atomic JSONL snapshots.
- `store/classification.rs`: pure representation priority, content identity,
  file-display parsing, and compact projection.
- `store/preview.rs`: bounded HTML/rich/media detail generation from owned
  seeds, with no SQLite dependency and no local path in IPC payloads.
- `store/database.rs`: SQLite orchestration, migrations, paging, seeds,
  retention, and compatibility delegates to focused store modules.
- `store/settings.rs`, `store/schema.rs`, and `store/models.rs`: typed settings,
  versioned schema declarations, and command-facing payloads.
- `tray.rs`: summary-only menu construction and tray actions.

## Startup And Process Ownership

`main.rs` does not open the database or create clipboard threads. It parses:

- `--copy-stack-history-jsonl <path>` (or `=<path>`);
- `--copy-stack-history-jsonl-max-data-bytes <bytes>` (default `4096`);
- the internal `--copy-stack-autostart` flag.

`lib.rs` registers the single-instance plugin first. A duplicate process calls
only the existing-process callback, which shows, unminimizes, and focuses the
main window. It does not create another database connection, tray, listener, or
consumer.

First-instance setup:

1. registers autostart without enabling it;
2. shows the main window for a manual launch or hides it for an autostart
   launch;
3. prepares the private database path and opens SQLite;
4. applies required schema/classifier migrations and retention;
5. starts and seeds the optional history-mirror worker;
6. installs shared state and localized native UI;
7. creates the tray;
8. starts the clipboard listener and storage threads.

Autostart state is authoritative in the operating system and is not duplicated
in SQLite. A write is always followed by a read-back; disagreement is a
verification error. The default remains disabled until the user opts in.

## Shared State And Locking

```rust
pub struct AppState {
    db: Mutex<Database>,
    pending_restore_suppression: Mutex<Option<PendingRestoreSuppression>>,
    history_mirror: Option<HistoryMirror>,
    tray_refresh: Option<TrayRefreshScheduler>,
    diagnostics: DiagnosticLog,
}
```

The database lock protects one `rusqlite::Connection`. Keep it limited to SQL
and copying owned seeds:

- `get_copy_events_page` and tray sync select only summary columns;
- `get_history_detail` reads a seed, releases the lock, then decodes and reads
  validated local media;
- restore commands read a seed and release the lock before decoding and
  writing the pasteboard;
- mirror scheduling sends a row-free refresh signal after commit; after
  debounce the worker reads current committed rows through its own read-only
  SQLite connection and performs decoding and filesystem I/O there.

Mutex poison and database failures are mapped to structured errors rather than
panicking across the command boundary.

## Command Handlers

### History reads

`get_copy_events_page(cursor?, page_size?)` returns a stable cursor page of
bounded `HistorySummary` values. Default size is 50 and maximum size is 100.
The response also carries total visible count and total accounted bytes.

`get_history_detail(content_hash)` reads one owned seed, builds at most 32
preview segments outside the lock, and enforces an 8 MiB serialized response
budget. Bounded image bytes are returned for in-memory `blob:` previews. Video
details return only a display label and media type; the asset protocol stays
disabled and full local paths never cross IPC.

### History mutations and restore

`delete_copy_event` and `clear_all_events` commit SQLite first, release the
lock, schedule an optional mirror refresh, and sync the tray. The frontend
reloads after these commands.

`copy_to_clipboard` uses the same canonical restore helper as the tray:

1. load the stored body and protocol metadata;
2. project to text when compact mode is enabled;
3. remove all old source/remote markers;
4. add exactly one source marker and, when stored, one remote marker;
5. write the event after releasing the database lock.

When restore-to-top is off, a five-second one-shot suppression prevents the
listener echo from changing order. When it is on, the row receives a new
timestamp, the mirror is scheduled, the tray is synced, and
`clipboard-history-updated` is emitted.

Once the operating-system pasteboard write succeeds, restore returns success.
Any later ordering, mirror, tray, or notification failure is reported as the
non-retryable `restore_post_processing_failed` event, so a UI retry cannot
repeat an already-completed external write.

### Settings and diagnostics

`get_app_settings` returns:

- `max_items` and `max_history_bytes`;
- `history_count`, `history_bytes`, and `history_limit_bytes`;
- `max_event_bytes`;
- menu visibility, restore ordering, compact mode;
- persisted and resolved language.

Mutators are `set_max_items`, `set_max_history_bytes`,
`set_show_in_menu_bar`, `set_move_restored_item_to_top`, `set_compact_mode`,
and `set_language`. Item limits accept 1–1000. The byte command accepts
16 MiB–4 GiB. Lower limits run cleanup before notifying History and the tray.

`get_autostart_status` and `set_autostart_enabled` operate on the OS login item
and return verified state.

`get_safe_diagnostics` returns at most 32 records. Each record contains only
timestamp, app version, platform, architecture, enumerated error code,
operation, and retryability.

## Capture Pipeline

For every listener event:

1. reject an empty event;
2. assess protocol markers across every item;
3. stop immediately for concealed, transient, or autogenerated content;
4. apply resource budgets;
5. if an oversized rich event has a valid bounded plain-text representation,
   degrade to that text while preserving source/remote markers; otherwise emit
   `capture-rejected` with only resource kind and size bucket;
6. read compact-mode state;
7. classify and encode the accepted event once;
8. apply restore suppression by normalized content identity;
9. insert/update and enforce retention transactionally;
10. schedule the optional row-free mirror refresh after commit;
11. coalesce rapid capture-driven tray refreshes, rebuild the summary-only
    tray, and emit `clipboard-history-updated`.

Protocol policy always precedes content hashing, preview generation, resource
classification, persistence, mirror export, and UI/tray presentation. See
`docs/design/nspasteboard-protocol.md`.

## Resource Budgets

- encoded event: 32 MiB, 64 items, 128 data entries per item, and 1024 bytes
  per type name;
- plain text: 4 MiB;
- HTML: 2 MiB;
- RTF: 4 MiB;
- image capture flavor: 16 MiB;
- file URL: 64 KiB;
- persisted display: bounded by the selected content-type capture limit; list
  summary: 512 bytes;
- preview image: 4 MiB and PNG dimension cap: 20 million pixels;
- detail: 32 segments and 8 MiB serialized;
- default accounted history budget: 256 MiB.

Length fields are checked before allocation while decoding persisted event
blobs.

## JSONL Worker

With the JSONL flag enabled, mutations schedule monotonically numbered row-free
refresh signals. Scheduling does no history scan, BLOB clone, decode,
serialization, or file I/O. A 200 ms debounce coalesces bursts. The worker then
opens an independent read-only SQLite connection and loads the latest committed
state, so delayed or reordered signals cannot carry a stale snapshot.

The worker validates the private destination, creates an exclusive `0600`
same-directory temporary file, writes rows in history order, flushes and
`fsync`s it, rechecks generation, atomically renames it, and syncs the parent
directory. A failed or superseded generation never truncates the last valid
snapshot. Exit flush/shutdown is bounded to two seconds.

The mirror is a sensitive copy of accepted history, including protocol metadata
and truncated body fields. It is not a log and must not be committed or shared
without sanitization.

Compact mode applies the same deduplicated text-only projection to History, the
tray, restore, and JSONL. Toggling compact mode schedules a mirror refresh.

## CSP And Capabilities

Production CSP denies external connections, unsafe script/style execution,
objects, forms, base URLs, and embedding. Development-only localhost/eval/style
allowances live in `devCsp`. Prototype freezing is enabled.

`main.json` grants only history commands and event listening.
`settings.json` grants only settings/autostart commands and event listening.
The unused opener dependency and permission are absent.

## Backend Change Checklist

- Keep protocol assessment before all content inspection.
- Keep summary queries and local media I/O outside one another's lock scope.
- Schedule the mirror only after a successful database commit.
- Return stable structured command errors; never forward raw errors or paths.
- Update Rust/TypeScript types, autogenerated command permissions,
  capabilities, and docs together.
- Run Rust format/check/test, frontend checks for contract changes, and the
  relevant manual matrix in `docs/security-release-checklist.md`.
