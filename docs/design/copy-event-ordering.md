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

The dedupe key is derived from clipboard content that is stable across copies.

- Prefer plain-text clipboard representations such as UTF-8 and UTF-16 text.
- Use decoded text content, normalized for whitespace, as the primary hash input.
- Fall back to deterministic raw clipboard data only when no stable text-like representation exists.
- Do not hash the entire serialized event payload because it can contain ancillary clipboard formats whose bytes vary even when the user-facing content is the same.

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
  `content_hash`/`event_data`/`timestamp` schema;
- convert formatted timestamps to Unix millisecond timestamps;
- recompute normalized content hashes for existing rows;
- remove older duplicates that collapse to the same normalized hash.

After migration, all future ordering and deduplication operations follow the persisted rules above.
