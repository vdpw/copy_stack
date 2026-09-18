# Pinned History

## Product Behavior

Pinning keeps frequently reused clipboard items easy to reach and protects them
from bulk clearing and automatic retention. It is stored per content identity,
survives restart and duplicate capture, and does not change the clipboard.

| Action               | Pinned item behavior                                                 |
| -------------------- | -------------------------------------------------------------------- |
| Browse or search     | Pinned group first; normal timestamp order within it                 |
| Pin/Unpin            | Persist desired state without changing timestamp                     |
| Clear history        | Keep pinned rows                                                     |
| Item/byte cleanup    | Keep pinned rows, including when they alone exceed limits            |
| Explicit item delete | Delete the selected item, even if pinned                             |
| Duplicate capture    | Refresh payload while preserving pin and position                    |
| Restore              | Keep pin; optional restore-to-top operates within the group          |
| Unpin                | Return to ordinary history and enforce current retention immediately |

Pinned rows count toward item and byte totals. If they alone consume the budget,
new unpinned captures may be removed immediately. Settings explains both this
exception and the effect of unpinning; the limits are not a hard cap on pins.

## Main Window And Settings

Every History card has a visible Pin/Unpin button, localized accessible label,
and pressed state. A pinned badge and group heading keep state understandable
without relying on color. The button stops card expansion, disables while the
request is pending, and refreshes the authoritative list after success. Search
uses the same groups and actions. No separate favorites page, pin order setting,
or drag reordering is introduced.

Settings labels the destructive action as clearing unpinned history, confirms
that pins will be kept, and reloads actual aggregate counts after completion.
The underlying `clear_all_events` command name remains unchanged.

Both window pages use shared macOS-inspired typography, rounded grouped
surfaces, and restrained translucent navigation/control layers. Clipboard
content remains readable, native window chrome stays native, and light/dark,
keyboard focus, reduced-motion, transparency, and contrast behavior must be
checked in desktop QA. This styling is implemented in the webview rather than
claiming a platform-native material API.

## Menu Bar

The native menu puts a labeled Pinned section first, with a pin marker on each
entry, followed by a separate Recent History section. Clicking an entry still
restores it directly, and supported hover previews keep their existing behavior.
Pin management stays in the main window to keep the native menu compact.

`menu_bar_item_limit` counts both groups together, with pinned entries taking
priority. `0` means all with the existing 1000-entry ceiling. Open History
remains available when some entries fall beyond the limit.

## Persistence And Compact Mode

Schema v4 adds `is_pinned INTEGER NOT NULL DEFAULT 0` constrained to `0` or `1`.
Migration from v3 preserves content and timestamps and defaults existing rows
to unpinned. Gated metadata rebuilds preserve pins even when identities merge.

History and search order by pin descending, timestamp descending, and hash
ascending. Cursor version 2 includes all three values. Frontend mutations reload
pages instead of applying a separate client-only ordering rule.

Compact mode treats equivalent text as one visible item. The summary is pinned
if any backing row is pinned; Pin/Unpin and explicit deletion operate on the
entire equivalent-text group. A later compact capture consolidates the rows and
retains any pin and the newest matching timestamp. Pinning does not make
otherwise ineligible image/file/video items visible in compact mode.

`set_copy_event_pinned` commits state, applies retention for Unpin, and then
refreshes the mirror, native menu, and main window. The command uses the
`pin_history` error operation and the existing bounded error contract.

## Verification

Automated coverage should exercise migration defaults/preservation, pin-first
paging and search boundaries, duplicate and compact consolidation, clear,
count/byte retention, over-budget pins, and immediate cleanup after Unpin.
Frontend checks cover the serialized flag, action state, and localized labels.

Real desktop checks must cover native menu ordering/limits/restore, restart,
focused and unfocused refresh, keyboard interaction, both window pages, and
appearance/accessibility settings. The detailed checklist is in
`docs/development.md`; it describes required checks and is not evidence that
they have already passed.
