# Development Workflow

## Prerequisites

- Node.js 18 or newer and pnpm.
- Rust stable.
- macOS/Tauri build dependencies.

The checked-in Rust manifest uses the published crate:

```toml
copy_event_listener = "0.1.2"
```

No sibling checkout or local path dependency is required. The release workflow
keeps a defensive legacy path-to-published replacement step, but it is a no-op
for the current manifest.

Rust Tauri is pinned to `2.11.4` and `tauri-build` to `2.6.3`; keep the pair
compatible during patch upgrades.

## Install And Run

```bash
pnpm install
pnpm dev
pnpm desktop:dev
```

Vite uses port 5173 with `strictPort: true`.

To keep manual debug QA away from the user's real history, create a private
directory in macOS's per-user temporary area and point the debug build at a
child directory:

```bash
CLIPECHO_QA_RUN_DIR="$(mktemp -d)"
COPY_STACK_QA_DATA_DIR="$CLIPECHO_QA_RUN_DIR/data" pnpm desktop:dev
```

This override is compiled only with debug assertions. Relative paths are
rejected, the resulting `clipecho.db` still passes the private-file checks,
and release builds use `$HOME/.clipecho` for bootstrap configuration and the
default database. Storage changes within a QA run persist under this isolated
bootstrap root. Do not substitute a
fixed child directly under `/tmp` or `/private/tmp`: their public immediate
parent is intentionally rejected. Reuse the generated directory for one QA
session, then remove it after the app exits.

Optional JSONL flags:

```bash
COPY_STACK_QA_DATA_DIR="$CLIPECHO_QA_RUN_DIR/data" pnpm desktop:dev -- \
  -- \
  --copy-stack-history-jsonl "$CLIPECHO_QA_RUN_DIR/clipecho-history.jsonl" \
  --copy-stack-history-jsonl-max-data-bytes 4096
```

The mirror is a coalesced asynchronous snapshot, not a synchronous append log.
It can contain accepted clipboard bodies and protocol metadata.

`--copy-stack-autostart` is reserved for the OS login item. Use it manually only
to test hidden-at-login policy.

## Build And Automated Checks

```bash
pnpm type-check
pnpm lint
pnpm test
pnpm build
pnpm security-check
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm desktop:build
```

The normal CI matrix runs frontend checks/build, the security guardrail, Rust
format/check/test, and dependency resolution natively on both `macos-15`
(Apple Silicon) and `macos-15-intel` (Intel). This catches architecture-specific
compile and unit-test failures, but it is not evidence that native UI,
NSPasteboard, login-item, permissions, or fault scenarios were exercised.

## Performance Harness

Run deterministic 100/1000-item text and mixed fixtures in release mode:

```bash
scripts/perf-history.sh
```

The harness uses synthetic private temporary data and prints JSON timing,
payload, storage, and row-count records. See `docs/performance.md`; do not turn
machine-dependent timings into ordinary unit-test thresholds.

## Verification Matrix

| Change                          | Required verification                                               |
| ------------------------------- | ------------------------------------------------------------------- |
| Frontend                        | type-check, lint, frontend tests, build                             |
| Rust backend                    | Rust format, check, and tests                                       |
| Command/event contract          | frontend and Rust checks plus desktop QA                            |
| Capture/restore/protocol        | Rust tests plus real macOS NSPasteboard QA                          |
| Persistence/migration/retention | Rust tests, legacy DB/rollback QA, second-start fast path           |
| Paging/detail/tray performance  | structural tests, performance harness, desktop scroll/detail QA     |
| Private files/JSONL             | Rust fault tests and native permission/failure QA                   |
| CSP/capabilities/errors         | `pnpm security-check`, frontend tests, offline/malicious-preview QA |
| Single instance/autostart       | lifecycle tests and packaged/manual macOS QA                        |
| Release                         | every automated gate and completed security release matrix          |
| Documentation only              | links, paths, command names, and `git diff --check`                 |

## Required Manual Desktop QA

The following is a checklist, not a record of completed testing:

1. Copy synthetic text and confirm History and the menu bar update.
2. Copy the same text again and confirm no duplicate and no order change.
3. Page through at least 100 items; expand formatted/image/video details and
   confirm detail is requested only on expansion.
4. Search for text beyond the first loaded page, mixed-case text, a file name,
   and one- or two-character CJK text. Verify pagination, `Command+F`, Escape,
   live refresh, compact mode, restore, and delete while filtered. Use the menu
   bar Search action and confirm the main-window field is focused.
5. Trigger a focused live update while scrolled and with cards expanded; verify
   the scroll anchor and expansion state remain stable. Repeat while the window
   is unfocused and verify a new copy returns History to the newest row.
6. Restore from History and the menu bar with both ordering settings. Verify
   long menu labels copy directly without opening a submenu.
7. Inspect the restored type list and confirm exactly one canonical source
   marker and remote marker only when applicable.
8. Exercise every synthetic NSPasteboard marker combination in
   `docs/design/nspasteboard-protocol.md`; skipped content must not appear in
   SQLite, History, tray, diagnostics, or JSONL.
9. Exercise oversized formatted/image/event fixtures. Confirm safe text
   degradation or a localized rejection notice, with no oversized IPC payload.
   Set Maximum item size to 64 MiB and capture a synthetic 33 MiB item. Lower
   it to 32 MiB: new oversized captures must be rejected, while the existing
   item remains restorable without a rejection notice from its own echo.
   Restart and verify the configured limit persists. Check blank, fractional,
   zero, and greater-than-256 MiB input validation.
10. Lower item and byte limits and confirm oldest unpinned rows are trimmed
    while pinned rows survive. Repeat with pinned rows alone exceeding each
    limit; totals may stay above the configured limit. Unpin an old row and
    confirm the current retention rules apply immediately.
11. Pin and unpin synthetic rows from History and search results. Verify pinned
    grouping, the badge/button state, keyboard activation, disabled state while
    saving, and no unintended expansion or clipboard write. Re-copy a pinned
    row, restore it with both ordering settings, and restart; verify its pin
    survives and ordering stays within its group. Request deletion of a pinned
    row; verify default Cancel focus, Escape cancellation, and explicit
    confirmation before deletion. Unpinned deletion remains immediate.
    Clear from Settings and then the menu bar; verify pinned rows
    survive in SQLite, the main window, the menu, and the optional JSONL mirror.
12. Toggle compact mode, menu visibility, restore ordering, and all languages.
    With menu visibility off, copy a synthetic item and confirm it is saved
    without attempting preview installation or showing a global runtime-error
    banner. Turn visibility on again and confirm the native item and hover
    preview return. Set the menu count to 20 and then 0; verify 20 and all
    retained rows up to the 1000-item ceiling appear, with pinned entries first,
    a pin marker, and separate section labels. With more pins than the limit,
    confirm Open History reaches the remaining entries. Verify direct-click
    restore and hover preview in both menu groups and across their separator.
13. Close the main window and click the macOS Dock icon; confirm the existing
    window shows and focuses again. Then start a duplicate process and confirm
    the same window activates while only one owner/listener/tray remains.
14. Open Settings from the macOS application menu and with `Command+,`; confirm
    the tray Settings entry still works. Verify a left sidebar with a return
    button above General, Appearance, Clipboard, and Menu Bar, and the selected
    configuration on the right. Switch categories, then use the sidebar's top
    button to return to History from each category.
15. Enable and disable launch at login and reopen Settings to verify OS state.
16. Launch with the autostart flag and confirm the main window stays hidden
    while capture and the menu bar remain active.
17. Verify `.clipecho` is `0700` and database/sidecars/mirror are `0600`.
18. Inject unsafe/unwritable private paths and slow/failing JSONL writes; verify
    safe errors, committed database mutations, complete last snapshot, and
    bounded exit.
19. Run offline and confirm history, settings, restore, and previews make no
    external request.
20. Run malicious HTML preview fixtures and verify scripts, navigation, forms,
    external resources, and unsafe URLs do not execute.
21. With more than 50 mixed pinned/unpinned rows, page across the group boundary
    in History and search; verify no duplicates or omissions. Pin a row from a
    later page and verify authoritative refresh while preserving the active
    query. In compact mode, use equivalent plain/HTML/RTF text with mixed pin
    flags; verify one pinned summary, group-wide Pin/Unpin and deletion, and
    retained pin after a new compact capture consolidates the rows.
22. Open a sanitized schema-v3 fixture and confirm migration to v4 defaults all
    existing pins to false without changing payloads or timestamps. Pin rows,
    restart twice, and verify persisted flags and ordinary startup behavior.
23. Inspect History and Settings in light/dark appearance, reduced motion,
    reduced transparency, increased contrast, and narrow window sizes. Verify
    text readability, keyboard focus, hover/pressed/disabled controls, all three
    languages, and native title-bar controls. The shared visual style must not
    obscure clipboard content or destructive-action confirmation.
24. In General, verify language and launch at login. In Appearance, verify theme.
    In Clipboard, verify compact capture, restore order, count/total/per-item limits, and
    Clear Unpinned. In Menu Bar, verify visibility and menu item limit. Check all
    four categories by keyboard and pointer, localized labels, scroll behavior,
    and the two-column layout at
    the supported minimum window size.
25. Select Light and Dark while the system uses the opposite appearance; verify
    both main-window pages and native window appearance honor the explicit
    choice. Return to System and change the macOS appearance while the app is
    open; verify both layers follow it. Restart after each preference and
    confirm persistence. Start from an existing database without a `theme` key
    and confirm the default is System without rewriting clipboard history.
    These are required manual checks, not a completed validation record.
26. Expand single and mixed file/folder clips in both themes. Verify each full
    path appears under its name in smaller, secondary-color text, long paths
    wrap, and collapsed cards show names only. Repeat for Finder reference
    URLs and missing paths; unavailable paths must not be invented.
27. In Clipboard settings, change Storage location to an empty owned folder.
    Verify history, pins, settings, search, restore, subsequent captures, and
    the optional JSONL mirror still work; the old database must be removed.
    Restart with the same QA root and verify the selected path persists.
    Cancel the picker, select the current directory, then try a directory with
    `clipecho.db` or a SQLite sidecar, and an unwritable directory. Failures must
    retain the old displayed/configured path and usable history, show a localized
    reason, and never overwrite existing destination files. Exercise config-write
    and source-delete failures in the Rust fault tests.

Record the full Apple Silicon and Intel evidence matrix in
`docs/security-release-checklist.md` before release. Native dual-architecture CI
does not mark that manual matrix complete.

## Local Database Inspection

```bash
sqlite3 "$HOME/.clipecho/clipecho.db" "PRAGMA user_version;"
sqlite3 "$HOME/.clipecho/clipecho.db" "SELECT key, value FROM app_metadata ORDER BY key;"
sqlite3 "$HOME/.clipecho/clipecho.db" "SELECT key, value FROM settings ORDER BY key;"
sqlite3 "$HOME/.clipecho/clipecho.db" "SELECT substr(content_hash, 1, 12), data_type, is_pinned, byte_count, timestamp FROM clipboard_events ORDER BY is_pinned DESC, timestamp DESC, content_hash ASC LIMIT 10;"
stat -f '%Sp %N' "$HOME/.clipecho" "$HOME/.clipecho/clipecho.db"
```

Use sanitized copies for migration testing. Never commit or attach real
databases, sidecars, JSONL mirrors, clipboard payloads, source identifiers, or
user file paths.

## Generated And Local Files

Do not commit `node_modules/`, `dist/`, `src-tauri/target/`,
`src-tauri/gen/`, SQLite files, logs, mirror files, or performance output
containing non-synthetic paths.

## Common Change Patterns

### Command Or Payload

Update the Rust command/type, `src/types.ts`, invoking hook, autogenerated
permission, window capability, and relevant docs. Keep list summaries separate
from detail payloads.

### Persistence Or Ordering

Read the ordering and pinned-history design records, plus the protocol record
when relevant. Bump the schema or classifier metadata version as
appropriate, make migration transactional, verify rollback and a second current
startup, and preserve cursor ordering.

### Capture Or Restore

Keep event-wide protocol assessment first. Reuse the canonical restore helper
for History and tray paths. Test compact/full mode and both restore-order
settings on real NSPasteboard.

### Tray

Keep the tray query summary-only, honor `menu_bar_item_limit` (`0` means all,
with a 1000-row ceiling), and do not introduce event decoding or local media
reads during menu construction. Include pinned entries in the same limit and
keep their leading section and marker. The macOS hover panel may perform only its
existing single-row, 64 KiB display lookup; verify that it preserves line
breaks, stays beside the menu on each display, and disappears when the menu
closes.

### Localized UI

Update the TypeScript `Messages` interface and all three catalogs. Update Rust
native catalogs when menu/window text changes. Verify both open webviews and
native menus update together.

### Security Or Release

Run `pnpm security-check`, review `docs/security-release-checklist.md`, and keep
manual evidence explicitly pending until it has actually been recorded.
