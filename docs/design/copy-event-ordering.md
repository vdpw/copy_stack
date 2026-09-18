# Copy Event Ordering And Deduplication

## Context

`Copy Stack` stores clipboard history in SQLite and renders the list in the Tauri UI. The history model must satisfy these product rules:

1. A newly observed copy event must appear at the top of ordinary history,
   following the pinned section.
2. When a user restores an existing item from the event list, the persisted
   record moves to the top of its pinned or ordinary group only if restore
   ordering is enabled.
3. If copied content already exists in history, the existing record must be
   updated without creating a duplicate or changing its order.
4. Duplicate detection must use stable key content because raw clipboard payloads may include volatile metadata such as time values.
5. Pins persist independently of content and timestamps. Pin/Unpin changes the
   group without resetting a row's position within its timeline.

## Design

### Persisted identity and ordering

The database uses the normalized `content_hash` as the primary key on
`clipboard_events`, and uses an integer Unix millisecond `timestamp` as the
ordering key within the pinned or ordinary group.

- The UI and search order is defined by
  `ORDER BY is_pinned DESC, timestamp DESC, content_hash ASC`.
- New records are inserted with the current Unix millisecond timestamp.
- Re-copying an existing record refreshes `event_data` but preserves that
  record's timestamp and pin flag.
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

- Prefer `public.rtf`, then `public.png`, then `public.html`.
- For a single local-image `public.file-url`, hash the file URL bytes and
  classify/display it by image extension, such as `png` or `heic`.
- For a single `public.file-url`, hash the file URL bytes and classify the item
  as `folder` when the URL ends with `/`; otherwise classify it as `file`.
- For multiple copied files/folders, require every item to have
  `public.file-url`, ignore other data types for classification, concatenate
  the file URL bytes in item order, and hash the concatenation. Classify as
  `files`, `folders`, or `files and folders` based on whether no URLs, all
  URLs, or some URLs end with `/`.
- For plain text copies, require one item with `public.utf8-plain-text`, then
  hash the raw text bytes and use the decoded text for display. Extra clipboard
  flavors in the same item are retained in `event_data`.
- Do not hash the entire binary event payload or arbitrary fallback data.

This keeps duplicate detection focused on the meaningful clipboard content and avoids false misses caused by time-bearing metadata.

When compact mode is enabled, identity is always the hash of the accepted
plain-text bytes. Older RTF/HTML rows are compared and visibly deduplicated by
that effective text. Capturing the same text again consolidates matching older
format rows into a single text-only row while preserving the newest matching
timestamp and any existing pin. Compact display aggregates equivalent rows as
pinned if any member is pinned, and Pin/Unpin affects the entire displayed
group. See `docs/design/pinned-history.md` for protection and UI behavior.

### UI refresh contract

The frontend should not prepend or reorder rows optimistically. Instead:

- backend clipboard writes update SQLite first;
- backend emits a refresh signal after persistence changes;
- frontend reloads the first page from `get_copy_events_page`.
- pin mutations follow this same contract and refresh before further paging.

This guarantees the visible order always matches the persisted order.

## Migration

Existing databases use a versioned transactional migration only when the SQL
schema or classifier metadata version is older:

- rewrite legacy `id` and `sort_order` tables into the current
  `content_hash`/`event_data`/`data_type`/`display`/`timestamp` schema;
- convert formatted timestamps to Unix millisecond timestamps;
- recompute normalized content hashes and display metadata for existing rows;
- remove older duplicates that collapse to the same normalized hash.
- remove unsupported rows that only survived through an old fallback pick.

Legacy `sort_order` is migration input only. The migration translates that exact
relative order into adjacent timestamps and then removes the column. Current
databases use `PRAGMA user_version` plus classifier metadata and do not rebuild
history on every startup.

After migration, all future ordering and deduplication operations follow the
persisted rules above. Schema v4 adds `is_pinned` with a false default for older
rows. History and search reads use stable `(is_pinned, timestamp, content_hash)`
cursors (`v2:<0|1>:<timestamp>:<hash>`) rather than offsets.
