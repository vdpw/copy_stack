# Copy Event Ordering And Deduplication

## Context

`Copy Stack` stores clipboard history in SQLite and renders the list in the Tauri UI. The history model must satisfy these product rules:

1. A newly observed copy event must appear at the top of the event list.
2. When a user restores an existing item from the event list, the persisted
   record moves to the top only if restore ordering is enabled.
3. If copied content already exists in history, the existing record must be
   updated without creating a duplicate or changing its order.
4. Duplicate detection must use stable key content because raw clipboard payloads may include volatile metadata such as time values.

## Design

### Persisted identity and ordering

The database uses the normalized `content_hash` as the primary key on
`clipboard_events`, and uses an integer Unix millisecond `timestamp` as the
ordering key.

- The UI order is defined by `ORDER BY timestamp DESC, content_hash ASC`.
- New records are inserted with the current Unix millisecond timestamp.
- Re-copying an existing record refreshes `event_data` but preserves that
  record's timestamp.
- Copying a duplicate from outside the app updates the existing row payload
  instead of inserting a second row or moving it.
- Restoring an item updates its timestamp only when `move_restored_item_to_top`
  is enabled.
- Timestamp writes use the greater of the current Unix millisecond timestamp and
  `MAX(timestamp) + 1` to preserve stable order for events that arrive in the
  same millisecond.

This makes list order a database concern rather than a frontend-only effect.

### Stable content hash

The dedupe key is derived from the highest-priority supported clipboard
representation, and the same classifier also stores `data_type` and
binary `display` preview bytes for the UI.

- Prefer `public.rtf`, then `public.html`, then `public.png`.
- For a single local-image `public.file-url`, hash the file URL bytes and
  classify/display it by image extension, such as `png` or `heic`.
- For a single `public.file-url`, hash the file URL bytes and classify the item
  as `folder` when the URL ends with `/`; otherwise classify it as `file`.
- For multiple copied files/folders, require every item to have
  `public.file-url`, ignore other surviving data types, concatenate the file URL
  bytes in item order, and hash the concatenation. Classify as `files`,
  `folders`, or `files and folders` based on whether no URLs, all URLs, or some
  URLs end with `/`.
- For plain text copies, require one item with only `public.utf8-plain-text`,
  then hash the raw text bytes and use the decoded text for display.
- Do not hash the entire binary event payload or arbitrary fallback data.

This keeps duplicate detection focused on the meaningful clipboard content and avoids false misses caused by time-bearing metadata.

### UI refresh contract

The frontend should not prepend or reorder rows optimistically. Instead:

- backend clipboard writes update SQLite first;
- backend emits a refresh signal after persistence changes;
- frontend reloads the event list from `get_copy_events`.

This guarantees the visible order always matches the persisted order.

## Migration

Existing databases need a lightweight migration:

- rewrite legacy `id` and `sort_order` tables into the current
  `content_hash`/`event_data`/`data_type`/`display`/`timestamp` schema;
- convert formatted timestamps to Unix millisecond timestamps;
- recompute normalized content hashes and display metadata for existing rows;
- remove older duplicates that collapse to the same normalized hash.
- remove unsupported rows that only survived through an old fallback pick.

After migration, all future ordering and deduplication operations follow the persisted rules above.
