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
  event_data: string;
  timestamp: number;
}
```

`event_data` is a JSON string containing `copy_event_listener::event::Event`.
`timestamp` is a Unix millisecond timestamp. The frontend only decodes enough of
the payload to show a preview.

Current preview interfaces:

```ts
interface Data {
  type: string;
  data: number[];
}

interface Item {
  data_list: Data[];
}

interface ClipboardEvent {
  items: Item[];
}
```

Keep these interfaces aligned with Rust serialization and the upstream
`copy_event_listener` event shape.

## Clipboard Preview Decoding

`getEventContent(eventData)` parses `event_data` and searches item data for:

- `public.utf8-plain-text`
- `public.utf16-plain-text`
- `NSStringPboardType`

UTF-16 values are decoded with `TextDecoder("utf-16le")`; other text values use
the default decoder. Empty or unsupported content falls back to a data type
label such as `[public.file-url]`.

`truncateContent(...)` normalizes whitespace and limits long previews to 160
characters.

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
