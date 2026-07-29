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

To keep manual debug QA away from the user's real history, point the debug
build at a dedicated absolute directory:

```bash
COPY_STACK_QA_DATA_DIR=/private/tmp/copy-stack-qa pnpm desktop:dev
```

This override is compiled only with debug assertions. Relative paths are
rejected, the resulting `copy_stack.db` still passes the private-file checks,
and release builds continue to use `$HOME/.copy_stack`.

Optional JSONL flags:

```bash
pnpm desktop:dev -- \
  -- \
  --copy-stack-history-jsonl /tmp/copy_stack_history.jsonl \
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

| Change | Required verification |
| --- | --- |
| Frontend | type-check, lint, frontend tests, build |
| Rust backend | Rust format, check, and tests |
| Command/event contract | frontend and Rust checks plus desktop QA |
| Capture/restore/protocol | Rust tests plus real macOS NSPasteboard QA |
| Persistence/migration/retention | Rust tests, legacy DB/rollback QA, second-start fast path |
| Paging/detail/tray performance | structural tests, performance harness, desktop scroll/detail QA |
| Private files/JSONL | Rust fault tests and native permission/failure QA |
| CSP/capabilities/errors | `pnpm security-check`, frontend tests, offline/malicious-preview QA |
| Single instance/autostart | lifecycle tests and packaged/manual macOS QA |
| Release | every automated gate and completed security release matrix |
| Documentation only | links, paths, command names, and `git diff --check` |

## Required Manual Desktop QA

The following is a checklist, not a record of completed testing:

1. Copy synthetic text and confirm History and the menu bar update.
2. Copy the same text again and confirm no duplicate and no order change.
3. Page through at least 100 items; expand formatted/image/video details and
   confirm detail is requested only on expansion.
4. Trigger a live update while scrolled and with cards expanded; verify the
   scroll anchor and expansion state remain stable.
5. Restore from History and the menu bar with both ordering settings.
6. Inspect the restored type list and confirm exactly one canonical source
   marker and remote marker only when applicable.
7. Exercise every synthetic NSPasteboard marker combination in
   `docs/design/nspasteboard-protocol.md`; skipped content must not appear in
   SQLite, History, tray, diagnostics, or JSONL.
8. Exercise oversized formatted/image/event fixtures. Confirm safe text
   degradation or a localized rejection notice, with no oversized IPC payload.
9. Lower item and byte limits and confirm oldest rows are trimmed.
10. Delete one item and clear all from both History and the menu bar.
11. Toggle compact mode, menu visibility, restore ordering, and all languages.
12. Start a duplicate process and confirm the existing window activates while
    only one owner/listener/tray remains.
13. Enable and disable launch at login and reopen Settings to verify OS state.
14. Launch with the autostart flag and confirm the main window stays hidden
    while capture and the menu bar remain active.
15. Verify `.copy_stack` is `0700` and database/sidecars/mirror are `0600`.
16. Inject unsafe/unwritable private paths and slow/failing JSONL writes; verify
    safe errors, committed database mutations, complete last snapshot, and
    bounded exit.
17. Run offline and confirm history, settings, restore, and previews make no
    external request.
18. Run malicious HTML preview fixtures and verify scripts, navigation, forms,
    external resources, and unsafe URLs do not execute.

Record the full Apple Silicon and Intel evidence matrix in
`docs/security-release-checklist.md` before release. Native dual-architecture CI
does not mark that manual matrix complete.

## Local Database Inspection

```bash
sqlite3 "$HOME/.copy_stack/copy_stack.db" "PRAGMA user_version;"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT key, value FROM app_metadata ORDER BY key;"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT key, value FROM settings ORDER BY key;"
sqlite3 "$HOME/.copy_stack/copy_stack.db" "SELECT substr(content_hash, 1, 12), data_type, byte_count, timestamp FROM clipboard_events ORDER BY timestamp DESC, content_hash ASC LIMIT 10;"
stat -f '%Sp %N' "$HOME/.copy_stack" "$HOME/.copy_stack/copy_stack.db"
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

Read both design records. Bump the schema or classifier metadata version as
appropriate, make migration transactional, verify rollback and a second current
startup, and preserve cursor ordering.

### Capture Or Restore

Keep event-wide protocol assessment first. Reuse the canonical restore helper
for History and tray paths. Test compact/full mode and both restore-order
settings on real NSPasteboard.

### Tray

Keep the hard 20-item summary query and do not introduce event decoding or local
media reads during menu construction.

### Localized UI

Update the TypeScript `Messages` interface and all three catalogs. Update Rust
native catalogs when menu/window text changes. Verify both open webviews and
native menus update together.

### Security Or Release

Run `pnpm security-check`, review `docs/security-release-checklist.md`, and keep
manual evidence explicitly pending until it has actually been recorded.
