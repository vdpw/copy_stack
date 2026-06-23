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
  content_hash TEXT PRIMARY KEY,
  event_data BLOB NOT NULL,
  data_type TEXT NOT NULL,
  display BLOB NOT NULL,
  timestamp INTEGER NOT NULL
);
```

Columns:

- `content_hash`: normalized SHA-256 hash used as the stable row key and
  deduplication key.
- `event_data`: compact binary clipboard event payload. The binary format stores
  each item, data type, and raw `data` bytes directly, including private or
  platform-specific clipboard flavors.
- `data_type`: backend classification used by the UI and tray, currently
  `rtf`, `html`, image extensions, `video`, `file`, `folder`, `files`,
  `folders`, `files and folders`, `text`, or `unsupported`.
- `display`: backend-selected preview bytes. Current text labels are stored as
  UTF-8 bytes. File and folder events store UTF-8 JSON with format
  `copy_stack.file-items.v1` and one `{type, name}` entry per copied item.
  PNG image events store the PNG bytes used by the frontend thumbnail preview.
- `timestamp`: Unix timestamp in milliseconds. This is also the ordering key.

Indexes:

```sql
CREATE INDEX IF NOT EXISTS idx_clipboard_events_timestamp
ON clipboard_events(timestamp DESC);
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

1. Creates `clipboard_events` with `content_hash` as the primary key for clean
   databases.
2. Migrates legacy `id`/`sort_order`/RFC3339 timestamp/JSON payload tables into
   the current schema when needed.
3. Creates indexes.
4. Creates `settings`.
5. Inserts default setting rows if they do not exist.
6. Calls `rebuild_history_metadata()`.

## Metadata Rebuild

`rebuild_history_metadata()` keeps databases compatible with the current
content-hash key and dedupe model.

Then it:

1. Reads all rows.
2. Recomputes a content hash from `event_data`.
3. Keeps the first row for each hash.
4. Rewrites the table with `content_hash`, binary `event_data`, `data_type`,
   binary `display`, and integer `timestamp`.

This means opening an older database can delete duplicate rows that collapse to
the same normalized content hash. Rows that cannot be classified by the current
supported clipboard rules are also dropped instead of being kept through the old
raw-payload fallback.

## Event Insertion

`insert_event(event)`:

1. Encodes the event to the binary clipboard payload format.
2. Classifies the event into `content_hash`, `data_type`, and `display`.
3. If a row with the same hash exists, updates `event_data`, `data_type`, and
   `display` while preserving the existing `timestamp`.
4. If it does not exist, computes the next history timestamp in Unix
   milliseconds and inserts a new row keyed by `content_hash`.
5. Runs `cleanup_old_events()` after new inserts.

Duplicate clipboard content refreshes the stored payload without creating
another row or changing list order.

## Rich Preview Payload

`get_copy_events` decodes stored `event_data` into a `rich_preview` array for
user-facing previews. WeCom-style rich clips expose `public.utf8-plain-text`
with the object replacement character (`U+FFFC`) where inline images appear.
The backend splits that text into segments and replaces each placeholder with a
thumbnail segment loaded from a supported `public.file-url` image, preserving
source order such as text-image, image-text, and text-image-text. Single local
video file URLs produce a video segment with label, media type, and decoded
local path so the UI can render a thumbnail without storing video bytes in the
command payload.

The raw clipboard event remains the source of truth for restore operations.
`rich_preview` is display-only and falls back to the existing `data_type` /
`display` preview when no ordered mixed preview can be built.

## JSONL History Mirror

When the app starts with `--copy-stack-history-jsonl <path>`, it rewrites that
file as a JSONL snapshot of the current `clipboard_events` table. Each line is
one database row ordered the same way as `get_copy_events`:

```json
{
  "content_hash": "...",
  "data_type": "text",
  "timestamp": 1710000000000,
  "display": {
    "byte_len": 11,
    "truncated": true,
    "encoding": "utf8",
    "value": "hell"
  },
  "event_data": {
    "items": [
      {
        "data_list": [
          {
            "type": "public.utf8-plain-text",
            "data": {
              "byte_len": 11,
              "truncated": true,
              "encoding": "utf8",
              "value": "hell"
            }
          }
        ]
      }
    ]
  }
}
```

`event_data` is decoded from the binary blob into clipboard items and data
flavors. Each byte field is represented as `{byte_len, truncated, encoding,
value}`. Valid UTF-8 is written as `encoding: "utf8"`; binary data is written as
lowercase hex. Values longer than
`--copy-stack-history-jsonl-max-data-bytes` are truncated before serialization;
the default limit is `4096` bytes per field.

The mirror is rewritten after startup cleanup and after history mutations. It
is intended for local inspection/debugging and can contain sensitive clipboard
data.

## Content Hashing

Deduplication uses a backend classifier that prefers normalized content hashes
for known clipboard types and falls back to hashing the full encoded event for
unsupported clipboard types. The classifier applies these priorities:

1. `public.rtf`: hash that `data` value; use `public.utf8-plain-text` as
   `display` when present; classify as `rtf`.
2. `public.png`: hash that `data` value; use the PNG bytes as `display`;
   classify as `png`.
3. `public.html`: hash that `data` value; use `public.utf8-plain-text` as
   `display` when present; classify as `html`.
   TODO: render HTML previews in the UI.
4. One `items` element with `public.file-url` for a local video path: hash the
   file URL `data`; classify as `video`; use the decoded file basename as
   `display`. This handles app-originated video copies that also expose an empty
   `public.tiff` flavor.
5. One `items` element with `public.file-url` for a local image path: hash the
   file URL `data`; classify with the lowercased file extension such as `png`,
   `jpg`, `tiff`, or `heic`; use the uppercased extension as `display`.
   This covers local-app image copies that expose `public.file-url` and may also
   include `public.tiff`.
6. One `items` element with `public.file-url`: hash the file URL `data`; store
   a structured file display payload with the item `type` (`file` or `folder`)
   and display `name`. The name comes from raw `public.utf8-plain-text` split on
   carriage returns when possible, then a safe basename fallback, then `File N`
   or `Folder N`; Finder reference ids such as `id=...` are never used as
   display names. A file URL ending with `/` is classified as `folder`;
   otherwise it is classified as `file`.
7. Multiple `items` elements where every item has `public.file-url`: ignore
   other data types for classification, concatenate all `public.file-url` data
   values in item order, and hash the concatenated bytes. Store a structured
   file display payload with one `{type, name}` entry per item using the same
   safe display name rules as single file URLs. Classify as `files` when no
   file URL ends with `/`, `folders` when all file URLs end with `/`, and
   `files and folders` when the event contains both.
8. Plain text copies: when there is exactly one `items` element with
   `public.utf8-plain-text`, hash that raw `data` value and store the same bytes
   as `display`; classify as `text`. Other data types in the same item are
   retained in `event_data` but not used for the plain text hash.
9. Unsupported copies: hash the encoded full event payload, classify as
   `unsupported`, and store a short display label listing the first clipboard
   data types. The full raw event payload is still stored in `event_data`, so
   the JSONL mirror includes all original data flavors even when the UI has no
   specialized preview for them.

Legacy rows that only survived because of the old arbitrary fallback are
reclassified with the current rules during metadata rebuild.

## Ordering

`timestamp` is the ordering field. New rows and explicit move-to-top updates use
the greater of the current Unix millisecond timestamp and `MAX(timestamp) + 1`
so history order remains stable even when events arrive in the same millisecond.
Duplicate insert updates preserve the existing timestamp.

```sql
SELECT COALESCE(MAX(timestamp), 0) FROM clipboard_events
```

History reads use:

```sql
ORDER BY timestamp DESC, content_hash ASC
```

Restoring an item moves it to the top only when
`move_restored_item_to_top` is true. Otherwise the backend suppresses the
listener echo and leaves ordering unchanged.

## Retention

`cleanup_old_events()` reads `max_items` from settings and deletes excess rows
from the bottom of history:

```sql
DELETE FROM clipboard_events WHERE content_hash IN (
  SELECT content_hash FROM clipboard_events
  ORDER BY timestamp ASC, content_hash DESC
  LIMIT ?1
)
```

Retention runs on startup, after `set_max_items`, and after inserting a new row.

## Manual Inspection

Useful local commands:

```bash
sqlite3 "$HOME/.copy_stack/copy_stack.db" ".schema"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT substr(content_hash, 1, 12), data_type, hex(substr(display, 1, 24)), timestamp FROM clipboard_events ORDER BY timestamp DESC LIMIT 20;"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT key, value FROM settings ORDER BY key;"
```

Do not commit copied database files. Clipboard payloads can contain sensitive
user data.

## Persistence Change Checklist

- Test with an existing database, not only a clean database.
- Preserve or deliberately migrate legacy `event_data` JSON compatibility.
- Keep dedupe behavior aligned with `docs/design/copy-event-ordering.md`.
- Keep tray and frontend refresh behavior aligned with persisted ordering.
- Keep the optional JSONL mirror aligned with history mutations.
- Run `cargo check --manifest-path src-tauri/Cargo.toml`.
- Run `pnpm type-check` if API payload shapes consumed by React changed.
