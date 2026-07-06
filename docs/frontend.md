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
- `src/App.css`: page layout, cards, buttons, responsive behavior, and modal
  styles.
- `vite.config.ts`: stable Tauri dev server on port `5173`.
- `eslint.config.js`: ESLint flat config for TypeScript and React.

## UI Views

`App.tsx` has two views:

- `history`: list of stored clipboard events with refresh, restore, delete, and
  clear-all actions.
- `settings`: local retention, restore ordering, menu bar visibility, and status
  information.

The active view is local React state:

```ts
type View = "history" | "settings";
```

The tray can switch views by emitting `app:navigate` with `history` or
`settings`.

## Tauri Commands Used By The Frontend

`App.tsx` calls these backend commands:

- `get_copy_events`: loads stored history.
- `get_app_settings`: loads `max_items`, `show_in_menu_bar`, and
  `move_restored_item_to_top`.
- `set_max_items`: updates retention limit and then reloads history.
- `set_show_in_menu_bar`: toggles tray visibility.
- `set_move_restored_item_to_top`: toggles restore ordering behavior.
- `delete_copy_event`: deletes one history row and reloads history.
- `copy_to_clipboard`: restores one stored event and reloads history.
- `clear_all_events`: deletes all history rows and reloads history.

If a command signature changes in Rust, update the corresponding frontend
argument object. Tauri maps camelCase frontend keys to snake_case Rust
parameters, for example `maxItems` to `max_items`.

## Tauri Events Used By The Frontend

`App.tsx` subscribes on mount:

- `clipboard-history-updated`: calls `loadEvents()`.
- `app:navigate`: sets the active view.

The component stores unlisten callbacks and calls them during effect cleanup.

## Data Shapes

The frontend receives stored rows as:

```ts
interface StoredEvent {
  content_hash: string;
  data_type: string;
  display: number[];
  timestamp: number;
  source_app: string | null;
}
```

SQLite keeps the source event as a binary blob for restore operations, but
`get_copy_events` does not return or decode `event_data`. `data_type` and
`display` are selected by the backend classifier and should be used for
user-facing previews. `display` is a byte array so text labels, structured
file/folder item metadata, and image thumbnail bytes can share the same field.
`timestamp` is a Unix millisecond timestamp. `source_app` is the best-effort
macOS foreground application name captured with the clipboard event and can be
`null` for older rows, permission failures, unsupported platforms, or app-owned
restore writes.

## Clipboard Preview Display

The history list decodes `StoredEvent.display` as UTF-8. Most data types store
plain text labels. File and folder events store JSON with format
`copy_stack.file-items.v1` and an `items` array whose entries contain `type`
(`file` or `folder`) and `name`; render one file/folder icon per item. Keep
preview selection in the backend classifier so the main window and tray menu use
the same display value.

History cards are folded by default. PNG image events whose `display` starts
with a PNG signature render a constrained thumbnail from a browser object URL;
the component revokes the URL on cleanup. The collapsed preview uses
`truncateContent(...)`, which defensively normalizes whitespace and limits long
previews to 40 display-width characters, counting CJK/full-width characters as
2 columns and ASCII characters as 1. Overflow uses `...`. Clicking a history
card expands it in place and shows the full decoded display content while the
restore and delete buttons keep their own actions. File and folder payloads are
also folded: the collapsed state shows one item with a remaining-count suffix,
and the expanded state shows the full item list. History card text is not
selectable, so repeated clicks only toggle expansion.

When `source_app` is present, the history card renders it beside the data type
badge. Missing sources are intentionally hidden rather than replaced with a
generic label.

TODO: render HTML previews in the UI for `data_type: "html"`.

## Settings Behavior

`max_items` is edited through a pending input value:

- Input must be an integer from 1 to 1000.
- The Apply button is enabled only when the value is valid and different from
  the current setting.
- Reducing below the current event count opens a confirmation modal.
- Confirming calls `set_max_items`, then reloads history.

The menu bar and restore-order settings are simple switch-style buttons backed
by Tauri commands.

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
- Treat clipboard payloads as sensitive; do not log actual content.
- Run `pnpm type-check`.
- Run `pnpm lint`.
- For command or event changes, validate with `pnpm desktop:dev`.
