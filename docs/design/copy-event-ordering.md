# Copy Event Ordering And Deduplication

## Context

`Copy Stack` stores clipboard history in SQLite and renders the list in the Tauri UI. The current implementation sorts by `timestamp` and deduplicates by hashing the full serialized clipboard event. That behavior does not match the product rules:

1. A newly observed copy event must appear at the top of the event list.
2. When a user copies an existing item from the event list, the persisted record must move to the top in SQLite, then the UI must refresh from SQLite.
3. If copied content already exists in history, the existing record must move to the top instead of creating a duplicate.
4. Duplicate detection must use stable key content because raw clipboard payloads may include volatile metadata such as time values.

## Design

### Persisted ordering

The database keeps an explicit `sort_order` column on `clipboard_events`.

- The UI order is defined by `ORDER BY sort_order DESC, timestamp DESC`.
- New records are inserted with the next highest `sort_order`.
- Re-copying an existing record updates that record’s `sort_order` to the current maximum plus one.
- Copying a duplicate from outside the app also updates the existing row’s `sort_order` instead of inserting a second row.

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

- add `sort_order` if missing;
- backfill `sort_order` from the current visible order;
- recompute normalized content hashes for existing rows;
- remove older duplicates that collapse to the same normalized hash.

After migration, all future ordering and deduplication operations follow the persisted rules above.
