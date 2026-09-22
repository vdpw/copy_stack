# Persistence

## Location And Private Files

The default database is:

```text
$HOME/.clipecho/clipecho.db
```

On the supported Unix/macOS path, startup:

- creates or tightens `.clipecho` to `0700`;
- creates or tightens the database and existing `-wal`, `-shm`, and `-journal`
  sidecars to `0600`;
- refuses symlinks, non-regular files, wrong-owner files, files with multiple
  hard links, insecure directories, and path identity changes;
- never falls back to a world-readable temporary or current directory.

Existing permission bits are only removed. Missing owner permissions are not
silently granted.

Settings > Clipboard > Storage location lets the user select a different
directory. The database filename remains `clipecho.db`. The bootstrap setting
stays at `$HOME/.clipecho/storage.json`, outside the movable database, so startup
can locate it. Debug `COPY_STACK_QA_DATA_DIR` overrides this bootstrap directory
as well as the initial database directory. The old `.copy_stack/copy_stack.db`
location is not automatically imported or deleted.

User-selected existing folders retain their permissions; the app validates
ownership and rejects unsafe writable directories, while database files remain
private `0600` files. A missing configured database, unreadable setting, or
unavailable volume fails startup instead of creating empty replacement history.

Relocation holds the shared database lock and excludes independent mirror
readers. It rejects an existing destination database or any `-wal`, `-shm`, or
`-journal` file, checkpoints SQLite, switches to rollback journaling, and creates
a consistent private snapshot with `VACUUM INTO`. This also supports moves
across filesystems. The new connection and integrity check must succeed before
configuration changes. History, pins, search, and settings move together.

A private `storage-move.json` recovery record is saved before changing the
bootstrap setting. Source-file removal is the commit point: failures before it
keep the original live connection and restore its configuration. If configuration
rollback is itself blocked, the recovery record keeps the original location
authoritative on restart. After commit the live connection and mirror use the
new database. No destination database is overwritten. Notifications after a
successful commit cannot turn the move into a reported failure.

## Current Schema

Schema version is stored in `PRAGMA user_version`. Classifier/derived-metadata
version is stored separately in `app_metadata`, because classifier policy can
change without an unrelated SQL shape change. The current schema version is 4.

```sql
CREATE TABLE clipboard_events (
  content_hash TEXT PRIMARY KEY,
  event_data BLOB NOT NULL,
  data_type TEXT NOT NULL,
  display BLOB NOT NULL,
  summary_display BLOB NOT NULL,
  summary_truncated INTEGER NOT NULL,
  compact_content_hash TEXT,
  compact_display BLOB,
  source_bundle_id TEXT,
  is_remote_clipboard INTEGER NOT NULL,
  byte_count INTEGER NOT NULL,
  timestamp INTEGER NOT NULL,
  metadata_version INTEGER NOT NULL,
  is_pinned INTEGER NOT NULL DEFAULT 0 CHECK (is_pinned IN (0, 1))
);

CREATE TABLE settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE app_metadata (
  key TEXT PRIMARY KEY,
  value INTEGER NOT NULL
);

CREATE VIRTUAL TABLE clipboard_event_search USING fts5(
  content_hash UNINDEXED,
  search_text,
  compact_search_text,
  tokenize = 'trigram'
);
```

Indexes support `(is_pinned DESC, timestamp DESC, content_hash ASC)` paging,
timestamp ordering, and canonical compact-text selection. The current schema does not contain `id`, `sort_order`,
or the removed legacy `source_app` heuristic.

## Stored Fields

- `content_hash`: lowercase SHA-256 identity of the selected public
  representation.
- `event_data`: bounded binary encoding of the accepted event used for restore.
- `data_type` / `display`: classified display metadata capped at 1 MiB;
  full restorable content remains in `event_data`.
- `summary_display`: at most 512 bytes for History and the menu bar.
- `summary_truncated`: tells the UI that the summary is incomplete.
- `compact_content_hash` / `compact_display`: effective plain-text projection
  for non-destructive compact-mode reads.
- `source_bundle_id`: exact valid UTF-8 source marker, including an explicit
  empty string; `NULL` means missing/invalid.
- `is_remote_clipboard`: Apple remote-clipboard marker presence.
- `is_pinned`: user-controlled protection from clear/retention and priority in
  History, search, and menu ordering. New and migrated rows default to false.
- `byte_count`: accounted event, display, summary, compact-display, and source
  bytes for retention. It is not a measurement of SQLite page overhead.
- `timestamp`: Unix milliseconds and the persisted ordering key.
- `metadata_version`: classifier metadata version used to derive the row.

## Settings

- `max_items`: default `100`, accepted UI range 1–1000.
- `max_history_bytes`: default `268435456` (256 MiB).
- `max_event_bytes`: default `33554432` (32 MiB), accepted range 1–256 whole
  MiB stored as bytes. Includes encoded-event overhead; restricts new capture
  only and never trims existing rows.
- `show_in_menu_bar`: default `true`.
- `menu_bar_item_limit`: default `0` (all retained rows), accepted UI range
  0–1000.
- `move_restored_item_to_top`: default `false`.
- `compact_mode`: default `false`.
- `language`: default `system`; other valid values are `en`, `zh-CN`, and
  `zh-TW`.
- `theme`: default `system`; other valid values are `light` and `dark`.
  Store the preference, not the currently resolved light/dark appearance, so
  system mode continues following operating-system changes after restart.

Autostart is not stored here. The operating system login item is authoritative.
Existing databases receive missing settings through default-setting
initialization; adding `theme` or `max_event_bytes` does not change the history
schema or rewrite existing clipboard rows. Invalid single-event byte settings
are rejected; writes must use whole MiB values within the codec safety ceiling.

## Versioned Initialization And Migration

Initialization and every pending migration run inside one immediate transaction:

1. read and reject unsupported future schema or derived-metadata versions;
2. create settings and metadata tables and insert missing defaults;
3. create the latest schema directly for an empty unversioned database;
4. bootstrap legacy/unversioned history to schema v2, then apply every explicit
   migration in order (`v2 -> v3 -> v4`, followed by future adjacent versions);
5. validate each target schema before advancing `PRAGMA user_version`;
6. rebuild separately-versioned classifier or search metadata only when stale;
7. validate the final tables, indexes, triggers, and search row accounting;
8. commit the complete chain, or roll all steps back to the original version.

The current fast path does not decode, reclassify, deduplicate, or rewrite all
history rows on every launch.

The `v3 -> v4` migration adds `is_pinned` with a false default without changing
existing payloads or timestamps. Gated classifier rebuilds retain pin flags;
when identities collapse, any pinned source keeps the resulting row pinned.

When a gated rebuild is required, a replacement table is created inside the
same transaction. Legacy JSON event payloads are converted to the bounded binary
format. Legacy `sort_order` is read only to preserve exact relative order, then
translated into adjacent timestamp ranks. Every row is reassessed by current
NSPasteboard policy and classification:

- concealed, transient, autogenerated, and unsupported rows are dropped;
- normalized duplicates collapse while preserving the first row in migrated
  order;
- source/remote metadata is derived from the event, never from legacy
  `source_app`;
- summaries, compact projections, byte counts, and metadata versions are
  rebuilt.

Row accounting and table/index validation run before the original table is
replaced. Any migration error rolls back the entire transaction; fault-injection
tests cover failures at multiple replacement stages.

Released migrations are forward-only and immutable. A database with a schema
version newer than the running app is preserved and rejected rather than
downgraded. Missing or outdated rebuildable search objects are recreated from
`clipboard_events`; unknown drift in the authoritative history schema is an
error and is never repaired by deleting user data.

## Search Index

Search uses the FTS5 trigram table, never image/media BLOBs or the restore
payload. `search_text` contains visible text and formatted-text projections plus
file, folder, and video names. `compact_search_text` contains only the effective
compact-mode text. Short one- and two-character queries use a fallback scan of
stored search text so CJK searches remain useful.

The index stores exactly one row per history row. Inserts and content updates
refresh the entry in the same transaction; delete/update triggers remove stale
entries during explicit deletion, clear, compact canonicalization, and
retention. `search_index_version` in `app_metadata` versions extraction/tokenizer
policy independently from SQL shape. The index is fully rebuildable from the
authoritative history table.

## Capture And Upsert

Protocol and resource policy run before persistence. `prepare_history_event`
then applies compact-mode projection when enabled, selects the supported
representation, encodes the event, derives summary/protocol/compact metadata,
and returns an owned prepared row.

Full-mode upsert:

- updates payload and derived metadata for an existing `content_hash` without
  moving it or resetting its pin flag;
- otherwise inserts at
  `max(current_unix_millis, MAX(timestamp) + 1)`.

Compact-mode upsert canonicalizes all rows with the same effective text into
one text-only row while preserving the newest matching timestamp and the
logical OR of their pin flags.

Every successful upsert enforces both retention limits in the same transaction.

## Hashing And Supported Content

Representation priority is:

1. `public.rtf`;
2. `public.png`;
3. `public.html`;
4. one local video `public.file-url`;
5. one local image `public.file-url`;
6. one generic file/folder URL;
7. multiple items when every item has a file URL;
8. one `public.utf8-plain-text` item.

Formatted and image hashes use the selected bytes. File/folder/video/image URL
hashes use the file URL bytes; multi-file identity concatenates URL bytes in
item order. Plain text uses its raw UTF-8 bytes. Private/protocol flavors do not
participate in identity, so source and remote metadata changes update one row
rather than creating duplicates.

Events with no supported public representation are not persisted.

## Summary Paging

`get_history_page` selects persisted summary columns only. It does not select or
decode `event_data` and does not read local media.

- default page size: 50;
- hard maximum: 100;
- cursor: `v2:<0|1>:<timestamp>:<lowercase-content-hash>`; the second field is
  the summary's pin state;
- order: `is_pinned DESC, timestamp DESC, content_hash ASC`;
- response totals: visible item count and accounted history bytes.

Cursor filtering compares the complete ordering tuple, so equal timestamps and
the pinned/unpinned boundary do not repeat or skip rows. Compact-mode paging
selects the newest row for each effective text, including across page
boundaries, and reports it pinned if any equivalent row is pinned. Search uses
the same ordering and cursor. A pin mutation refreshes pages from the start
rather than continuing with a cursor from the previous ordering.

## Menu Summary And Lazy Hover Preview

Tray construction selects only `content_hash`, `data_type`, `is_pinned`, and the
persisted 512-byte `summary_display` for the configured rows. Pinned summaries
come first and consume the same combined menu limit as ordinary summaries;
`0` means all up to the 1000-row ceiling. On macOS, highlighting one
text/HTML/RTF row triggers a separate query by content hash. That query selects
only the line-preserving plain-text projection when one exists, applies SQLite
`substr(...)` before the value reaches Rust, and returns at most 64 KiB plus a
truncation flag. It falls back to the classified display for legacy or
non-projectable formatted rows. Binary/media rows use only their summary and do
not open the text panel.

The hover path does not select or decode `event_data`, inspect local paths, or
read media. Only one highlighted row is materialized; closing the menu drops
the visible preview.

## Lazy Detail And Restore Seeds

`get_history_detail_seed` and `get_restore_seed` copy one row under the
database lock. Event decoding and local media inspection happen after the lock
is released.

Detail construction is display-only and bounded to 32 segments and 8 MiB.
Formatted HTML up to the 2 MiB rendering budget uses the same isolated
renderer. Values outside that budget use a bounded 1 MiB
plain-text fallback.
Local images must be ordinary files whose identity remains stable before,
during, and after a bounded read. PNG previews also enforce a 20-million-pixel
header limit. Video bytes and video paths are never copied through IPC; video
detail contains only a display label and media type. File and folder details
decode the stored event only on expansion, returning display names and full
paths within the same serialized response budget. Finder references resolve to
their current paths outside the database lock; unresolved paths remain null.
The persisted file summaries stay name-only and retain their 512-byte bound.

Restore uses the original encoded event (or its compact projection) plus stored
source/remote metadata. Canonical protocol markers are applied immediately
before the pasteboard write.

## Ordering

History order is:

```sql
ORDER BY is_pinned DESC, timestamp DESC, content_hash ASC
```

New inserts and explicit restore-to-top updates use a monotonic timestamp.
Duplicate capture updates preserve the old timestamp. When restore-to-top is
disabled, listener suppression preserves order. Pin/Unpin changes only the
group, not the timestamp; restore-to-top moves an item to the top of its own
group. Compact reads use an aggregate pin flag for equivalent text rows.

## Pin, Delete, And Clear

`set_event_pinned` commits the pin flag in a transaction. In compact mode it
updates the target and every row with the same effective text. Unpinning runs
retention before commit; an old unpinned item can therefore be removed
immediately. Explicit deletion remains allowed for pinned items; in compact
mode it removes the entire equivalent-text group so a hidden member does not
reappear as the next representative.

`clear_all_events` retains its command name but executes
`DELETE FROM clipboard_events WHERE is_pinned = 0`. Both Settings and the tray
use this behavior. Neither pinning nor clearing changes content identity.

## Count And Byte Retention

Cleanup first removes oldest unpinned rows beyond `max_items`, then recomputes
accounted bytes and removes oldest remaining unpinned rows until total
`byte_count` is at or below `max_history_bytes`, or no unpinned rows remain.
Pinned rows count toward both budgets but are never evicted by cleanup. If
pinned rows alone exceed a budget, they remain and newly captured unpinned rows
may be evicted immediately.

Cleanup runs:

- during first-instance startup;
- in every successful upsert transaction;
- after changing either retention limit.
- after unpinning an item.

The settings response exposes both current totals so Settings never needs to
load or count the full history list.

## Asynchronous Atomic JSONL Mirror

`--copy-stack-history-jsonl <path>` enables a sensitive local snapshot. After a
committed mutation, the application releases its database lock and schedules a
row-free refresh signal. After debounce, the mirror worker opens its own
read-only SQLite connection and reads the latest committed rows. Mutations
therefore never clone full-history BLOBs or perform mirror I/O under the shared
database lock, and a delayed signal cannot reintroduce stale rows.

The worker:

1. coalesces rapid mutations for 200 ms using monotonic generations;
2. validates and reads the private database through an independent connection;
3. applies the current full or compact-mode projection;
4. validates a private absolute destination and secure parent;
5. creates an exclusive `0600` temporary file in the destination directory;
6. decodes rows and writes one JSON object per line;
7. flushes and syncs the temporary file;
8. commits only if the generation is still newest;
9. atomically renames and syncs the parent directory.

A failure or superseded write leaves the previous complete snapshot intact.
Application exit requests a final flush and worker shutdown with a two-second
bound.

Each byte field is serialized as `{byte_len, truncated, encoding, value}`.
Valid UTF-8 uses `utf8`; other bytes use lowercase hex. The per-field value is
truncated to the configured byte count (default 4096). Accepted records may also
include `source_bundle_id` and `is_remote_clipboard`. Policy-skipped events have
no database row and therefore no JSONL line.

## Manual Inspection

```bash
sqlite3 "$HOME/.clipecho/clipecho.db" "PRAGMA user_version;"
sqlite3 "$HOME/.clipecho/clipecho.db" "SELECT key, value FROM app_metadata ORDER BY key;"
sqlite3 "$HOME/.clipecho/clipecho.db" "SELECT substr(content_hash, 1, 12), data_type, is_pinned, byte_count, timestamp FROM clipboard_events ORDER BY is_pinned DESC, timestamp DESC, content_hash ASC LIMIT 20;"
sqlite3 "$HOME/.clipecho/clipecho.db" "SELECT key, value FROM settings ORDER BY key;"
stat -f '%Sp %N' "$HOME/.clipecho" "$HOME/.clipecho/clipecho.db"
```

Do not attach a real database or JSONL mirror to tests, logs, issues, or release
evidence. Use synthetic fixtures.

## Persistence Change Checklist

- Test a clean database, legacy migration, rollback injection, and a second
  current-version startup.
- Preserve protocol filtering and derive metadata only from the event.
- Preserve cursor ordering and summary-only list/menu-construction queries.
- Keep macOS menu hover preview single-row, display-only, and bounded in SQL.
- Keep item and byte cleanup transactional.
- Schedule mirror I/O only after commit and outside the database lock.
- Run Rust tests, the performance harness where relevant, and the manual
  database/private-file scenarios in the release checklist.
