# Project Overview

## Product

Copy Stack is a local-first desktop clipboard manager built with Tauri, React,
TypeScript, Rust, and SQLite. It monitors clipboard changes, stores a bounded
history, lets the user restore previous items, and exposes recent items through
the macOS menu bar.

The app is designed around local state. Clipboard payloads are stored on the
user's machine in SQLite; there is no cloud sync, account system, or remote API
in the current codebase.

## Current Capabilities

- Capture clipboard events through `copy_event_listener`.
- Persist clipboard event payloads as binary blobs in SQLite.
- Render recent clipboard history in the main Tauri window.
- Restore a saved clipboard item through a Tauri command or tray menu item.
- Delete one event or clear all history.
- Configure the maximum number of stored events.
- Hide or show the app's menu bar entry.
- Configure whether restored items move to the top of history.
- Keep the app running when the main window is closed by hiding the window.
- Reopen the main window from the tray menu and navigate to History or Settings.

## Primary User Workflows

### Browse History

The user opens Copy Stack, sees stored clipboard events ordered by persisted
recency, reviews a preview of each event, and can refresh, restore, or delete
items.

### Restore Previous Content

The user clicks the restore button in the app or selects an item from the tray
menu. The backend decodes the stored `copy_event_listener::event::Event` blob
and writes it back to the system clipboard.

### Keep History Bounded

The user sets `max_items` in Settings. Lowering the limit asks for confirmation
in the UI, then the backend trims the oldest rows according to persisted
Unix millisecond timestamps.

### Menu Bar Access

The tray menu mirrors the current history. Each menu item restores the matching
stored event. The menu also provides History, Settings, Clear history, and Quit
actions.

## Non-Goals In Current Code

- Cross-device sync.
- Search, tags, categories, or filtering.
- Import/export.
- Global keyboard shortcuts.
- Rich previews for every clipboard format.
- Cross-platform tray behavior beyond the current Tauri configuration and
  macOS-focused copy in the UI.

## Data Privacy Model

Clipboard data is sensitive. The current app keeps history locally at
`$HOME/.copy_stack/copy_stack.db` and does not intentionally transmit clipboard
payloads. When adding features, treat clipboard content as private by default:

- Do not log clipboard payloads.
- Do not send clipboard content to third-party services.
- Avoid adding telemetry around item contents.
- Keep generated database files out of source control.

## Naming

- Product name: Copy Stack.
- Package name: `copy_stack`.
- Tauri identifier: `com.copy-stack.app`.
- Frontend command boundary: Tauri `invoke(...)` calls.
- Backend event boundary: Tauri `emit(...)` events.
- Clipboard event type: `copy_event_listener::event::Event`.
