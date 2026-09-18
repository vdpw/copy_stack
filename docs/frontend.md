# Frontend Guide

## Stack And Files

- React 18, strict TypeScript, Vite 6, and the Tauri JavaScript API v2.
- `src/App.tsx`: window-level composition, language synchronization, and theme
  application.
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

`App.tsx` owns the main window's `history` and `settings` page state. On macOS,
the native application-menu Settings item (`Command+,`) selects Settings
through `app:navigate`; the tray can also request Settings. Settings is rendered
in the same webview with a left navigation sidebar and a right configuration
area. The sidebar starts with a back-to-History button, followed by General,
Appearance, Clipboard, and Menu Bar categories. Category navigation changes the
configuration area without leaving Settings. When Settings is active, the
History view is unmounted and does not load clipboard history.

## Commands Used By History

- `get_copy_events_page({cursor, pageSize, query?})`
- `get_history_detail({contentHash})`
- `delete_copy_event({contentHash})`
- `set_copy_event_pinned({contentHash, pinned})`
- `copy_to_clipboard({contentHash})`
- `get_app_settings()`
- `get_safe_diagnostics()`

History requests 50 summaries at a time. The backend caps every request at 100.
When the trimmed query is nonempty, the same cursor contract pages through all
matching retained history, not only rows already loaded in React.
The main list observes a sentinel 320 pixels ahead of the viewport and loads
the next cursor page automatically as the user scrolls. The Load More button
remains available as an accessibility and unsupported-observer fallback.
Refresh reloads enough cursor pages to preserve the currently loaded depth and
replaces them only while its request generation remains current. Load-more
merges by content hash, ignores stale generations, coalesces concurrent
requests, and pauses automatic retries after an error.

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
- `set_menu_bar_item_limit({menuBarItemLimit})`
- `set_move_restored_item_to_top({moveRestoredItemToTop})`
- `set_compact_mode({compactMode})`
- `set_language({language})`
- `set_theme({theme})`
- `clear_all_events()`

Tauri maps camelCase frontend keys to snake_case Rust arguments. Update
`src/types.ts`, the invoking hook, Rust serialization, command permissions, and
the main-window capability together when a contract changes.

## Events

- `clipboard-history-updated`: refresh the first page and settings totals while
  preserving the History scroll anchor.
- `app:navigate`: select `history` or `settings`; a repeated History request
  also refreshes the visible list.
- `app:focus-search`: select History and focus/select its search field; the tray
  uses this after showing the main window.
- `app-language-changed`: reload authoritative settings in the main webview.
- `capture-rejected`: display a localized, dismissible resource-limit notice.
- `app-operation-error`: development-only surface for asynchronous startup,
  capture, tray, and post-restore diagnostics. Production builds do not
  subscribe to this global runtime-error channel.

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
  is_pinned: boolean;
  timestamp: number;
  byte_count: number;
  has_detail: boolean;
  search_preview: string | null; // bounded, present only for search results
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
  text_preview: string | null;
  rich_preview: RichPreviewSegment[];
}
```

The list response never contains raw `event_data`, full rich preview bytes, or
an unbounded display. Source provenance is retained for canonical restore but
is not rendered in History. Remote provenance remains a localized History
badge. Neither affects identity, and the menu bar omits both.

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

- formatted HTML uses the same renderer throughout the 2 MiB capture budget,
  is rebuilt into an allowlisted tree capped at 2,048 nodes and 24 levels,
  strips every image/resource URL, and maps allowlisted formatting to a fixed
  set of presentation classes without emitting inline `style` attributes;
- malformed or legacy HTML outside the capture budget falls back to at most
  1 MiB of escaped plain text in a fixed, vertically and horizontally
  scrollable code viewport instead of rendering an empty iframe;
- the sanitized document is rendered in an empty-sandbox iframe with
  `default-src 'none'` and `img-src 'none'`; the outer and inner policies
  authorize only the exact SHA-256 hash of the static preview stylesheet, so
  the same typography, document layout, code surface, and non-selection rules
  work under both development and production CSP without `unsafe-inline`;
  vertical and horizontal scrolling remain available inside the fixed
  viewport, simple single-line previews use a compact viewport, and multiline
  or structured content keeps the full height; preview interaction does not
  collapse the owning History card;
- image bytes use short-lived object URLs which are revoked on cleanup;
- mixed text/image segments preserve clipboard order;
- video detail is presented as metadata without loading the full video into
  React memory.

The production outer CSP also blocks external connections and unsafe
script/style execution. Do not relax either the outer or inner policy to make a
specific clipboard payload render. Any preview stylesheet change must update
the checked hash in both policies; the security gate and frontend tests reject
hash drift.

## Refresh And Interaction

History has a sticky search field. Input is debounced for 180 ms, capped at 256
characters, and sent to SQLite-backed search. `Command+F` focuses/selects the
field. Escape clears a nonempty query and otherwise blurs it. Search results
retain ordinary paging, expand, restore, pin, and delete behavior, and show a
localized result count and empty state. When a match falls outside the collapsed
summary, the result includes a bounded plain-text match excerpt. A live clipboard
update refreshes the active query instead of dropping back to unfiltered history.

The initial load shows a loading state. Later refreshes keep the list mounted,
capture the first visible card and its offset, replace the first page, then
restore that scroll anchor. The set of expanded hashes remains in view state.
When `clipboard-history-updated` arrives while the application window is not
focused, the refresh instead resets the page to the top after the new first
page commits, so returning to the app starts at the newest item.
When restore-to-top is enabled and the main window restores a row other than
the first one, History owns that refresh, waits for the reordered first page to
render, and then scrolls to the top with a 320 ms ease-out animation. Reduced
motion preferences use an immediate jump. History consumes the matching backend
update so the ordinary focused-window anchor restoration cannot undo the
scroll. Restoring the first row or restoring while the setting is disabled does
not issue a redundant reload.

History and search results show a Pinned group before Recent History. Within
each group, rows follow persisted `timestamp DESC, content_hash ASC` order.
Every card has an accessible Pin/Unpin action (`aria-pressed`), and pinned cards
have a visible badge. The action is disabled while its command is in flight;
success refreshes authoritative pages and announces the change to assistive
technology. Pinning does not copy the item, expand the card, or rewrite its
timestamp. Unpinning returns it to the ordinary timeline and immediately
applies retention, so it can disappear if it exceeds the current limits.

Restore, pin, and delete buttons stop card-toggle propagation. Each restore button is
disabled while its command is in flight, and a successful pasteboard write
shows short copy feedback even if later post-processing reports a non-retryable
failure. Load, detail, delete, pin, clear, restore, settings, and listener failures
are visible and retryable only when the backend marks them so.

## Settings Shape And Behavior

`get_app_settings` returns item and byte budgets, current item/byte totals,
maximum encoded event bytes, menu/restore/compact settings, persisted and
resolved languages, and the persisted `theme` preference. Settings uses those
aggregate values; it never counts History pages.

The left sidebar groups configuration into four categories:

| Category   | Controls in the right configuration area                                    |
| ---------- | --------------------------------------------------------------------------- |
| General    | Language and launch at login                                                |
| Appearance | System/Light/Dark theme                                                     |
| Clipboard  | Compact capture, restore ordering, item and byte limits, and Clear Unpinned |
| Menu Bar   | Menu bar visibility and menu bar item limit                                 |

The back-to-History control stays above the categories, making return navigation
available from every category. Category labels and every setting remain
localized. Keep keyboard focus and the active-category state distinguishable
without relying on color alone. The right area scrolls independently and resets
to the top when the category changes. Unapplied numeric drafts survive changes
to unrelated settings in other categories.

The document viewport stays fixed on every page and disables outer overscroll.
The settings sidebar and main pane each own an inner scroller with vertical
overscroll containment. Their backgrounds, the native-controls spacer, and the
page header stay outside those scrollers, so boundary feedback cannot pull the
whole window away from its background. History uses its own `.content-panel`
scroller beneath the fixed native-controls area; the pagination observer,
scroll-to-top animation, and scroll-anchor restoration all use that element.
Entering History focuses the scroll container without moving it, so native
keyboard paging works with the document locked. Search shortcuts focus/select
the sticky input without changing the reading position; Escape from an empty
search returns focus to the history scroller.

General, Appearance, and Menu Bar omit persistent per-control descriptions.
Category descriptions and actionable validation errors remain. Clipboard keeps
its behavior descriptions; the two storage controls show short explanations
behind adjacent question-mark buttons. Click to toggle help, click outside or
press Escape to dismiss it, and close it when leaving the category. The menu
item-count input keeps its zero-means-all explanation in a hover title.

Item count accepts 1 through 1000. Reducing it below `history_count` requires
confirmation. The history byte budget accepts 16 through 4096 MiB. Both limits
are enforced immediately by the backend against unpinned rows. Pinned rows
count toward totals but survive both limits, even when they alone exceed a
budget. Storage help explains the pin exception; unpinning applies the limits
immediately. Other SQLite-backed settings use an
optimistic value, invoke the command, re-read authoritative settings, and roll
back/reconcile after failure.

Autostart is separate from SQLite. The switch reads the operating system login
item, disables while reading/writing, verifies the value returned after a
mutation, and attempts another authoritative read after failure. Autostart
defaults to off.

The `system` language preference is resolved by the backend to `en`, `zh-CN`,
or `zh-TW`. `app-language-changed` keeps the in-window pages synchronized and
native menus are rebuilt by Rust.

The `theme` preference accepts `system`, `light`, or `dark` and defaults to
`system`. Selecting a theme persists it through `set_theme`, synchronizes native
window appearance, and applies it to both History and Settings. Explicit light
or dark selections override system appearance; system mode derives the webview
appearance from `matchMedia('(prefers-color-scheme: dark)')` and follows changes
while the app is running. Remove the media-query listener during effect cleanup.
Theme application belongs at the window level so leaving Settings does not
reset the selection.

Clear Unpinned is presented only on Settings in the webview. Its confirmation
explicitly states that pinned items are kept. The command name remains
`clear_all_events` for compatibility, but it deletes only unpinned rows. After
`clear_all_events` succeeds, Settings reloads the authoritative aggregate
counts; returning to History mounts a fresh first-page query.
Confirmation dialogs make the background inert, initially focus Cancel, contain
Tab navigation, support Escape, and return focus to the triggering control or
the nearest usable input after closing.

## Window Appearance

History and Settings share a macOS-inspired visual language: system typography,
rounded grouped surfaces, restrained translucency on navigation and controls,
and readable content backgrounds. Settings follows the macOS System Settings
references: a sidebar with colored category icons, a compact sticky title area,
and grouped rows with inset dividers. Light mode uses a gray sidebar, white
content area, and pale gray groups; dark mode uses warm neutral grays. The
sidebar selection is blue while focused and gray when unfocused. The theme
selector uses compact blue desktop and overlapping-window previews in
Light/Dark/System order, with labels beneath them and an offset blue selection
ring. System combines light and dark halves while retaining system-following
behavior. Settings-specific surface
tokens are scoped to its shell; content width is capped at 680px in wide windows.

The macOS window uses an overlay title bar with its native window buttons and
hidden native title. Settings reserves 44px above sidebar navigation; History
reserves a 40px draggable top strip. Explicit drag regions use Tauri's `deep`
behavior, which excludes interactive controls. The main-window capability adds
only `allow-start-dragging` and `allow-internal-toggle-maximize` for dragging and
double-click zoom. Native menus and window behavior remain owned by macOS.
Retain System/Light/Dark theme selection,
reduced-motion, and contrast support. Explicit theme colors take precedence over system
color-scheme queries; system mode should still respond to live appearance changes.
Validate focus rings, disabled states, pinned badges, destructive confirmation,
and long multilingual labels when changing the appearance.

## Error Boundary

Commands reject with:

```ts
interface CommandError {
  code: ErrorCode;
  operation: Operation;
  retryable: boolean;
}
```

`get_startup_error` is bootstrapped before settings or History invokes in every
build. Development builds also subscribe to `app-operation-error` for
asynchronous runtime diagnostics. `invokeCommand` validates enumerated fields
and converts every unknown value to a generic safe error.

Development error banners fetch the backend's bounded
`get_safe_diagnostics` records and offer an explicit copy action with visible
success/failure feedback. Production error banners keep actionable localized
command and startup feedback, but do not fetch or render diagnostic JSON and do
not surface background runtime failures through a global banner. Raw Rust,
database, filesystem, source-id, hash, path, HTML, and clipboard-content errors
are never displayed.

## Frontend Change Checklist

- Keep Rust types, `src/types.ts`, command arguments, permissions, and the
  main-window capability synchronized.
- Keep all three message catalogs complete.
- Preserve cursor paging and on-demand details; do not restore an all-history
  IPC response.
- Treat clipboard data as sensitive and never log it.
- Run `pnpm type-check`, `pnpm lint`, `pnpm test`, and `pnpm build`.
- Validate command, event, and native-window behavior with
  `pnpm desktop:dev`.
