# Frontend Guide

## Stack And Files

- React 18, strict TypeScript, Vite 6, and the Tauri JavaScript API v2.
- `src/App.tsx`: window-level composition and language synchronization.
- `src/features/history/`: History view, event cards, previews, and detail cache.
- `src/features/settings/`: Settings view.
- `src/hooks/useClipboardHistory.ts`: first-page refresh and cursor pagination.
- `src/hooks/useHistoryDetails.ts`: lazy detail requests.
- `src/hooks/useAppSettings.ts`: settings and autostart state.
- `src/api/tauri.ts`: typed invocation and structured error normalization.
- `src/lib/htmlPreview.ts`: HTML sanitization and the inner preview document.
- `src/lib/display.ts`: bounded display decoding, file labels, and byte units.
- `src/types.ts`: serialized command/event contracts.
- `src/i18n.ts`: English, Simplified Chinese, and Traditional Chinese catalogs.

`App.tsx` selects the `main` History surface or the separately created
`settings` surface from the current Tauri window label. The settings window does
not load clipboard history.

## Commands Used By History

- `get_copy_events_page({cursor, pageSize})`
- `get_history_detail({contentHash})`
- `delete_copy_event({contentHash})`
- `clear_all_events()`
- `copy_to_clipboard({contentHash})`
- `get_app_settings()`
- `get_safe_diagnostics()`

History requests 50 summaries at a time. The backend caps every request at 100.
Refresh reloads enough cursor pages to preserve the currently loaded depth and
replaces them only while its request generation remains current. Load-more
merges by content hash and ignores stale generations.

Detail is requested only when an expanded row reports `has_detail`. The
frontend cache coalesces concurrent requests for the same hash, ignores results
from a reset generation, and keeps at most 12 entries. Text and structured file
summaries expand without a detail command.

## Commands Used By Settings

- `get_app_settings()`
- `get_autostart_status()`
- `set_autostart_enabled({enabled})`
- `set_max_items({maxItems})`
- `set_max_history_bytes({maxHistoryBytes})`
- `set_show_in_menu_bar({showInMenuBar})`
- `set_move_restored_item_to_top({moveRestoredItemToTop})`
- `set_compact_mode({compactMode})`
- `set_language({language})`

Tauri maps camelCase frontend keys to snake_case Rust arguments. Update
`src/types.ts`, the invoking hook, Rust serialization, command permissions, and
both window capabilities together when a contract changes.

## Events

- `clipboard-history-updated`: refresh the first page and settings totals while
  preserving the History scroll anchor.
- `app:navigate`: refresh History when requested by a native menu.
- `app-language-changed`: reload authoritative settings in every webview.
- `capture-rejected`: display a localized, dismissible resource-limit notice.
- `app-operation-error`: surface startup, capture, tray, and post-restore
  failures through the same structured error UI.

Every listener is unregistered during effect cleanup. The app does not listen
for `new-copy-event`.

## History Shapes

```ts
interface HistorySummary {
  content_hash: string;
  data_type: string;
  display: number[]; // at most 512 persisted bytes
  display_truncated: boolean;
  source_bundle_id: string | null;
  is_remote_clipboard: boolean;
  timestamp: number;
  byte_count: number;
  has_detail: boolean;
}

interface HistoryPage {
  items: HistorySummary[];
  next_cursor: string | null;
  has_more: boolean;
  total_count: number;
  total_bytes: number;
}

interface HistoryDetail {
  content_hash: string;
  html_preview: string | null;
  rich_preview: RichPreviewSegment[];
}
```

The list response never contains raw `event_data`, full rich preview bytes, or
an unbounded display. Source and remote provenance are presentation metadata
and do not affect identity. The menu bar deliberately omits those badges.

`HistoryDetail` is bounded to 8 MiB and at most 32 segments. Image segments are
limited to supported formats and 4 MiB. Video segments carry validated metadata
and the UI currently renders a video label rather than transferring video
bytes.

## Preview Rendering

Collapsed cards use the persisted bounded summary. UTF-8 text is normalized and
truncated to 40 display columns, counting CJK/full-width characters as two.
File/folder summaries use `copy_stack.file-items.v1`; the collapsed state shows
one item plus a remaining count, and expansion shows the available item list.

Expanded eligible cards request detail:

- formatted HTML is accepted only up to 64 KiB, rebuilt into an allowlisted
  tree capped at 2,048 nodes and 24 levels, strips every image/resource URL,
  and keeps only allowlisted inline formatting;
- the sanitized document is rendered in an empty-sandbox iframe with
  `default-src 'none'` and `img-src 'none'`;
- image bytes use short-lived object URLs which are revoked on cleanup;
- mixed text/image segments preserve clipboard order;
- video detail is presented as metadata without loading the full video into
  React memory.

The production outer CSP also blocks external connections and unsafe
script/style execution. Do not relax either the outer or inner policy to make a
specific clipboard payload render.

## Refresh And Interaction

The initial load shows a loading state. Later refreshes keep the list mounted,
capture the first visible card and its offset, replace the first page, then
restore that scroll anchor. The set of expanded hashes remains in view state.
Restore does not issue a redundant immediate reload: if restore-to-top changes
ordering, the backend event triggers the refresh.

Restore and delete buttons stop card-toggle propagation. Each restore button is
disabled while its command is in flight, and a successful pasteboard write
shows short copy feedback even if later post-processing reports a non-retryable
failure. Load, detail, delete, clear, restore, settings, and listener failures
are visible and retryable only when the backend marks them so.

## Settings Shape And Behavior

`get_app_settings` returns item and byte budgets, current item/byte totals,
maximum encoded event bytes, menu/restore/compact settings, and persisted and
resolved languages. Settings uses those aggregate values; it never counts
History pages.

Item count accepts 1 through 1000. Reducing it below `history_count` requires
confirmation. The history byte budget accepts 16 through 4096 MiB. Both limits
are enforced immediately by the backend. Other SQLite-backed settings use an
optimistic value, invoke the command, re-read authoritative settings, and roll
back/reconcile after failure.

Autostart is separate from SQLite. The switch reads the operating system login
item, disables while reading/writing, verifies the value returned after a
mutation, and attempts another authoritative read after failure. Autostart
defaults to off.

The `system` language preference is resolved by the backend to `en`, `zh-CN`,
or `zh-TW`. `app-language-changed` keeps both webviews synchronized and native
menus are rebuilt by Rust.

## Error Boundary

Commands reject with:

```ts
interface CommandError {
  code: ErrorCode;
  operation: Operation;
  retryable: boolean;
}
```

`get_startup_error` and the `app-operation-error` listener are bootstrapped
before settings or History invokes. `invokeCommand` validates enumerated fields
and converts every unknown value to a generic safe error. Error banners fetch
the backend's bounded `get_safe_diagnostics` records and offer an explicit copy
action with visible success/failure feedback. Raw Rust, database, filesystem,
source-id, hash, path, HTML, and clipboard-content errors are not displayed.

## Frontend Change Checklist

- Keep Rust types, `src/types.ts`, command arguments, permissions, and window
  capabilities synchronized.
- Keep all three message catalogs complete.
- Preserve cursor paging and on-demand details; do not restore an all-history
  IPC response.
- Treat clipboard data as sensitive and never log it.
- Run `pnpm type-check`, `pnpm lint`, `pnpm test`, and `pnpm build`.
- Validate command, event, and native-window behavior with
  `pnpm desktop:dev`.
