# Persistence And Data Model

## Database Location

The database path is built in `Database::database_path()`:

```text
$HOME/.copy_stack/copy_stack.db
```

The app creates `$HOME/.copy_stack` if needed. Older docs may mention
`copy_stack.db` in the repo root; the active code now stores the database under
the user's home directory.

## Database Owner

`src-tauri/src/store/database.rs` owns schema creation, settings, history
queries, deduplication, ordering, retention, and lightweight migrations.

The `Database` struct wraps one `rusqlite::Connection`. Tauri stores it in
`AppState.db: Mutex<Database>`.

## Tables

### `clipboard_events`

```sql
CREATE TABLE IF NOT EXISTS clipboard_events (
  id TEXT PRIMARY KEY,
  event_data TEXT NOT NULL,
  timestamp TEXT NOT NULL,
  content_hash TEXT,
  sort_order INTEGER NOT NULL DEFAULT 0
);
```

Columns:

- `id`: UUID string generated on insert.
- `event_data`: serialized `copy_event_listener::event::Event` JSON.
- `timestamp`: RFC 3339 UTC timestamp.
- `content_hash`: normalized hash used for deduplication.
- `sort_order`: persisted ordering key.

Indexes:

```sql
CREATE UNIQUE INDEX IF NOT EXISTS idx_clipboard_events_content_hash
ON clipboard_events(content_hash)
WHERE content_hash IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_clipboard_events_sort_order
ON clipboard_events(sort_order DESC, timestamp DESC);
```

### `settings`

```sql
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
```

Current keys:

- `max_items`, default `100`.
- `show_in_menu_bar`, default `true`.
- `move_restored_item_to_top`, default `false`.

Settings are stored as strings and parsed by helper methods.

## Schema Initialization

`initialize_schema()` runs on every startup. It:

1. Creates `clipboard_events`.
2. Adds `content_hash` if missing.
3. Adds `sort_order` if missing.
4. Creates indexes.
5. Creates `settings`.
6. Inserts default setting rows if they do not exist.
7. Calls `rebuild_history_metadata()`.

## Metadata Rebuild

`rebuild_history_metadata()` makes existing databases compatible with the
current ordering and dedupe model.

It chooses an order:

- If any row already has `sort_order > 0`, it orders by
  `sort_order DESC, timestamp DESC`.
- Otherwise it orders by `timestamp DESC`.

Then it:

1. Reads all rows.
2. Recomputes a content hash from `event_data`.
3. Keeps the first row for each hash.
4. Deletes later duplicates.
5. Backfills `content_hash`.
6. Backfills `sort_order` so the first visible row receives the highest order.

This means opening an older database can delete duplicate rows that collapse to
the same normalized content hash.

## Event Insertion

`insert_event(event)`:

1. Serializes the event to JSON.
2. Computes a normalized content hash.
3. Computes `next_sort_order()`.
4. Checks whether a row with the same hash exists.
5. If it exists, updates `event_data`, `timestamp`, and `sort_order` on that
   row.
6. If it does not exist, inserts a new UUID row.
7. Runs `cleanup_old_events()` after new inserts.

Duplicate clipboard content moves to the top instead of creating another row.

## Content Hashing

Deduplication uses stable user-facing content rather than the full serialized
event JSON.

For each item, the database prefers text-like types:

- `public.utf8-plain-text`
- `public.utf16-plain-text`
- `public.plain-text`
- `public.text`
- `text/plain`
- `NSStringPboardType`
- `public.url`
- `public.file-url`
- `text/uri-list`

Text is decoded, null characters are removed, whitespace is normalized, and the
result is fed into SHA-256 with type separators.

If no preferred text-like type exists for an item, the fallback is the
lexicographically first data type and its raw bytes. If no fragments exist, the
fallback is the full serialized event JSON.

## Ordering

`sort_order` is the primary ordering field. New or duplicate events receive:

```sql
SELECT COALESCE(MAX(sort_order), 0) + 1 FROM clipboard_events
```

History reads use:

```sql
ORDER BY sort_order DESC, timestamp DESC
```

Restoring an item moves it to the top only when
`move_restored_item_to_top` is true. Otherwise the backend suppresses the
listener echo and leaves ordering unchanged.

## Retention

`cleanup_old_events()` reads `max_items` from settings and deletes excess rows
from the bottom of history:

```sql
DELETE FROM clipboard_events WHERE id IN (
  SELECT id FROM clipboard_events
  ORDER BY sort_order ASC, timestamp ASC
  LIMIT ?1
)
```

Retention runs on startup, after `set_max_items`, and after inserting a new row.

## Manual Inspection

Useful local commands:

```bash
sqlite3 "$HOME/.copy_stack/copy_stack.db" ".schema"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT id, timestamp, sort_order, substr(content_hash, 1, 12) FROM clipboard_events ORDER BY sort_order DESC, timestamp DESC LIMIT 20;"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT key, value FROM settings ORDER BY key;"
```

Do not commit copied database files. Clipboard payloads can contain sensitive
user data.

## Persistence Change Checklist

- Test with an existing database, not only a clean database.
- Preserve or deliberately migrate `event_data` JSON compatibility.
- Keep dedupe behavior aligned with `docs/design/copy-event-ordering.md`.
- Keep tray and frontend refresh behavior aligned with persisted ordering.
- Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- Run `pnpm type-check` if serialized shapes consumed by React changed.
