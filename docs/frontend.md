# Frontend Guide

## Stack

- React 18.
- TypeScript with `strict`, `noUnusedLocals`, and `noUnusedParameters`.
- Vite 6.
- Tauri JavaScript API v2.
- Icons from `lucide-react`.
- Styling in plain CSS.

## Main Files

- `src/main.tsx`: React entry point.
- `src/App.tsx`: application UI, state, Tauri commands, Tauri event listeners,
  clipboard payload preview decoding, settings UI, and destructive action
  confirmation.
- `src/i18n.ts`: supported language and preference types, locale normalization,
  the typed frontend message catalog, and language display names.
- `src/App.css`: page layout, cards, buttons, responsive behavior, and modal
  styles.
- `vite.config.ts`: stable Tauri dev server on port `5173`.
- `eslint.config.js`: ESLint flat config for TypeScript and React.

## UI Views

`App.tsx` renders one of two window-specific surfaces:

- The Tauri `main` window shows stored clipboard events with refresh, restore,
  delete, and clear-all actions.
- The separately created `settings` window shows language, local retention,
  restore ordering, menu bar visibility, and status information.

`App.tsx` selects the surface from `getCurrentWindow().label`; there is no
frontend view state. Tray actions show/focus the main window or create/focus the
settings window.

## Tauri Commands Used By The Frontend

`App.tsx` calls these backend commands:

- `get_copy_events`: loads stored history.
- `get_app_settings`: loads `max_items`, `show_in_menu_bar`,
  `move_restored_item_to_top`, `compact_mode`, `language`, and
  `resolved_language`.
- `set_max_items`: updates retention limit and then reloads history.
- `set_show_in_menu_bar`: toggles tray visibility.
- `set_move_restored_item_to_top`: toggles restore ordering behavior.
- `set_compact_mode`: toggles text-only storage, display, and restore behavior,
  then reloads history.
- `set_language`: accepts `system`, `en`, `zh-CN`, or `zh-TW`, updates native
  UI through the backend, and returns the complete updated `AppSettings`.
- `delete_copy_event`: deletes one history row and reloads history.
- `copy_to_clipboard`: restores one stored event and reloads history.
- `clear_all_events`: deletes all history rows and reloads history.

If a command signature changes in Rust, update the corresponding frontend
argument object. Tauri maps camelCase frontend keys to snake_case Rust
parameters, for example `maxItems` to `max_items`.

## Tauri Events Used By The Frontend

`App.tsx` subscribes on mount:

- `clipboard-history-updated`: calls `loadEvents()`.
- `app-language-changed`: reloads settings so every open webview applies the
  backend-resolved language.

The component stores unlisten callbacks and calls them during effect cleanup.

## Data Shapes

The frontend receives stored rows as:

```ts
interface StoredEvent {
  content_hash: string;
  data_type: string;
  display: number[];
  html_preview: string | null;
  rich_preview: RichPreviewSegment[];
  timestamp: number;
}
```

SQLite keeps the source event as a binary blob for restore operations, but
`get_copy_events` does not return the raw `event_data`. `data_type` and
`display` are selected by the backend classifier and remain the fallback
user-facing preview. `display` is a byte array so text labels, structured
file/folder item metadata, and image thumbnail bytes can share the same field.
`html_preview` contains the stored `public.html` representation when available
and compact mode is disabled. The frontend sanitizes it and renders it only in
an expanded card inside a sandboxed iframe.
`rich_preview` is a backend-decoded preview, with segments tagged as `text`,
`image`, or `video`; image segment bytes are intended for thumbnails, and video
segments carry local file metadata for Tauri asset rendering.
`timestamp` is a Unix millisecond timestamp.

Settings use this command response shape:

```ts
interface AppSettings {
  max_items: number;
  show_in_menu_bar: boolean;
  move_restored_item_to_top: boolean;
  compact_mode: boolean;
  language: string;
  resolved_language: string;
}
```

`language` is the persisted preference and may be `system`.
`resolved_language` is always one of the three concrete supported languages and
is the value used to render the UI.

## Clipboard Preview Display

The history list decodes `StoredEvent.display` as UTF-8. Most data types store
plain text labels, including video file basenames for `data_type: "video"`.
File and folder events store JSON with format `copy_stack.file-items.v1` and an
`items` array whose entries contain `type` (`file` or `folder`) and `name`;
render one file/folder icon per item. When the backend cannot recover a safe
filename, `name` is empty and the frontend generates a localized `File N` or
`Folder N` label from the semantic type and item position. Keep preview
selection in the backend classifier so the main window and tray menu use the
same display value.

History cards are folded by default. PNG image events whose `display` starts
with a PNG signature render a constrained thumbnail from a browser object URL;
the component revokes the URL on cleanup. Mixed text/image events use
`rich_preview` when present so text and image thumbnails render in original
clipboard order, including cases like text-image, image-text, and
text-image-text. Standalone local image file URLs also use `rich_preview`
because their `display` field contains an extension label rather than image
bytes. Expanded cards render rich image previews at a larger size.
Video events use `rich_preview` to render a local video thumbnail from metadata
instead of copying video bytes through the command payload. The collapsed
preview uses
`truncateContent(...)`, which defensively normalizes whitespace and limits long
previews to 40 display-width characters, counting CJK/full-width characters as
2 columns and ASCII characters as 1. Overflow uses `...`. Clicking a history
card expands it in place and shows the full decoded display content while the
restore and delete buttons keep their own actions. File and folder payloads are
also folded: the collapsed state shows one item with a remaining-count suffix,
and the expanded state shows the full item list. History card text is not
selectable, so repeated clicks only toggle expansion.

Expanded formatted-text cards prefer `html_preview`. Active elements, inline
event handlers, and resource/navigation URLs are removed before rendering.
The iframe sandbox and content security policy block scripts and external
resource loads while preserving inline text formatting and embedded data
images.

History reloads keep the current list mounted after the initial load. Restore
actions do not request a redundant frontend reload; the backend emits
`clipboard-history-updated` only when restore ordering changes the persisted
list. This avoids thumbnail flicker and preserves the window scroll position.
After a restore command succeeds, the clicked button briefly changes to a green
checkmark and the card receives a subtle highlight animation. An `aria-live`
message announces the successful copy, and the animations are disabled when
the user prefers reduced motion.

## Localization

The frontend supports English (`en`), Simplified Chinese (`zh-CN`), and
Traditional Chinese (`zh-TW`). `src/i18n.ts` keeps the corresponding types
separate:

```ts
type LanguagePreference = "system" | "en" | "zh-CN" | "zh-TW";
type SupportedLanguage = "en" | "zh-CN" | "zh-TW";
```

The translation catalog is a
`Record<SupportedLanguage, Messages>`. Add user-facing text to the `Messages`
interface and all three catalogs together so TypeScript reports missing
translations. Keep language codes synchronized with the backend implementation
in `src-tauri/src/i18n.rs`.

The backend response is authoritative after startup. The browser locale helper
provides only the initial/failure fallback before `get_app_settings` returns.
When the preference is `system`, the backend uses `sys-locale` and returns the
result in `resolved_language`. The frontend applies that concrete language to
the message catalog, localized timestamps, `document.documentElement.lang`,
and the settings-window document title.

Locale normalization treats `zh-Hant` and Chinese locales for Taiwan, Hong
Kong, or Macao as `zh-TW`; other Chinese locales resolve to `zh-CN`. English
locale variants resolve to `en`, and an unsupported system locale falls back to
English.

## Settings Behavior

`max_items` is edited through a pending input value:

- Input must be an integer from 1 to 1000.
- The Apply button is enabled only when the value is valid and different from
  the current setting.
- Reducing below the current event count opens a confirmation modal.
- Confirming calls `set_max_items`, then reloads history.

The menu bar, restore-order, and compact-mode settings are simple switch-style
buttons backed by Tauri commands. In compact mode, the history response contains
only recognizable text rows, including plain-text projections of older
formatted rows. Image/file-dominant rows are hidden.

The language select stores either the system-following preference or a concrete
language override. Changing it calls `set_language`, applies the returned
settings immediately, and relies on `app-language-changed` to synchronize any
other open webviews. Native application and tray menus are rebuilt by the
backend rather than translated in React.

## Styling Conventions

- Use existing utility class patterns in `src/App.css`.
- Keep controls responsive at the `1080px` and `720px` breakpoints.
- Prefer existing button variants: `btn-primary`, `btn-secondary`, and
  `btn-danger`.
- Preserve readable clipboard previews with `word-break: break-word`.
- Avoid introducing a component library unless the UI grows enough to justify
  the dependency.

## Frontend Change Checklist

- Keep command names and payload keys synchronized with Rust.
- Keep event names synchronized with `src-tauri/src/tray.rs`.
- Keep `LanguagePreference`, `SupportedLanguage`, and every `Messages` catalog
  synchronized when adding languages or UI text.
- Treat clipboard payloads as sensitive; do not log actual content.
- Run `pnpm type-check`.
- Run `pnpm lint`.
- For command or event changes, validate with `pnpm desktop:dev`.
