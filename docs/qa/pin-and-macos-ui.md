# Pin and macOS-style UI verification

Verified on 2026-09-18 in the development worktree. This is feature QA, not the
complete release or dual-architecture acceptance matrix.

## Automated checks

- `pnpm type-check`, `pnpm lint`, `pnpm build`, and `pnpm security-check`: passed.
- `pnpm test`: 52 tests passed across 16 files.
- Rust format/check passed; Rust tests: 160 passed, 1 ignored performance harness.
- `scripts/perf-history.sh`: the ignored release performance matrix passed for
  text/mixed fixtures with 100 and 1000 rows, including page sizes, complete
  pagination, menu row counts, and JSONL accounting.
- At 1000 rows, first-page reads measured 0.20–0.47 ms, complete paging 3.8–7.1 ms,
  and menu reads 0.29–0.31 ms across two runs. Byte retention measured 48–56 ms,
  above the older 15 ms reference in the performance document. This is not a
  timing test failure or a measured regression against the current base commit.
- A debug `.app` bundle built successfully with a separate QA application ID.

Coverage includes schema v3 migration/defaults/rollback/reopen; pin-aware
pagination and search; count/byte retention and clear; duplicate capture and
compact-mode consolidation; explicit compact deletion; tray limits and label
width; pin-decorated hover preview comparison; frontend command arguments,
request deduplication, failure/retry, scroll behavior, and accessible controls.

## Observed desktop checks

The app ran with `COPY_STACK_QA_DATA_DIR` in a private temporary directory and a
separate application identifier. The installed application's history was not
used for destructive test actions.

- Dark appearance: History and grouped Settings rendered at the default
  600 × 700 window size, including the clear-confirmation dialog.
- Pinning a synthetic record moved it to the pinned section, set the button's
  pressed state, kept content collapsed, and scrolled to the top.
- Unpinning the synthetic record returned it to the ordinary history group.
- The final accessibility tree exposed Pin, Copy, and Delete as independent
  controls next to the expandable content control.
- Clearing a six-record synthetic fixture from Settings left one pinned record;
  authoritative Settings totals and History both showed one remaining item.
- The same test database reopened through multiple debug-app launches.

## Remaining manual coverage

Native tray mouse selection/hover and the pinned/recent separator boundary were
not fully exercised through desktop automation. Ordering, limits, labels, and
hover text comparison have automated coverage. Light appearance, system
accessibility appearance preferences, and Intel hardware also remain manual QA
items; their CSS adaptations are implemented but this record does not claim
visual acceptance for those environments.
