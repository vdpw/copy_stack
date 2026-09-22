# v0.4.0 release record

Prepared on 2026-09-22 from main commit
`123c1d2eb958f3eb87e5df3ec871e16b193d889a` (merged PR #44).
The frontend manifest, Rust manifest and lockfile, and Tauri configuration
all use version `0.4.0`.

## Changes since v0.3.0

- Rename Copy Stack to ClipEcho and refresh the application and menu bar icons.
- Configure the maximum clipboard item size from 1 to 256 MiB (default 32 MiB).
  Lowering the limit preserves existing history and restore behavior.
- Confirm before permanently deleting a pinned item.
- Show full file and folder paths in expanded history cards.
- Suppress false oversized-item warnings when restoring previously saved large
  items after lowering the capture limit.

## Upgrade guidance

The database location and bundle identifier remain unchanged. Before replacing
Copy Stack, disable Launch at Login in the old app and quit it. Install and
open ClipEcho, then re-enable Launch at Login if desired. The product rename
changes the managed login-item name; the old login item is not automatically
migrated or removed.

## Verification

Local checks ran on Apple Silicon (`arm64`), macOS 27.0 (26A428), Rust 1.98.1,
and pnpm 10.33.0 with AC power. Other foreground workload was not controlled;
performance timings are observations, not portable guarantees.

- Frozen-lockfile frontend installation, security guardrails, type-check,
  lint, and production frontend build passed.
- All 90 frontend tests passed across 19 files.
- Rust formatting and check passed with the locked dependency graph; all 183
  ordinary Rust tests passed. The performance test is intentionally ignored
  by the ordinary suite.
- The separate release-mode fixture-v3 performance matrix passed for 100/1000
  text and mixed items, including pagination, tray, retention, and JSONL
  structural checks.

| Fixture | First page | Complete page walk | Tray query | Byte cleanup |
| --- | ---: | ---: | ---: | ---: |
| text / 100 | 0.124 ms | 0.114 ms | 0.063 ms | 2.586 ms |
| text / 1000 | 0.386 ms | 6.834 ms | 0.326 ms | 51.275 ms |
| mixed / 100 | 0.098 ms | 0.088 ms | 0.052 ms | 1.988 ms |
| mixed / 1000 | 0.214 ms | 3.870 ms | 0.283 ms | 51.023 ms |

The 1000-item byte-cleanup observations remain above the historical 15 ms
review signal and within the approximately 47–53 ms range documented for
v0.3.0. This release does not claim to resolve that existing bulk-cleanup cost.

The existing main commit passed both native CI jobs in
[run 35701675857](https://github.com/vdpw/ClipEcho/actions/runs/35701675857).

Earlier Apple Silicon feature observations are recorded in
[item limits, pinned deletion, and file paths](item-limits-and-file-paths.md).
They are feature-level debug-app evidence, not the complete packaged release
matrix.

## Release decision and remaining coverage

On 2026-09-22 the release owner instructed: "直接发布v0.4.0" (publish v0.4.0
directly). This release proceeds under that instruction with incomplete manual
coverage documented here. It does not carry forward the v0.3.0 exception or
claim that pending manual checks passed. The tag-triggered release workflow
retains all automated gates on native Apple Silicon and Intel runners.

The full packaged desktop matrix, native Intel clipboard and window behavior,
login/logout/reboot, application relocation, offline WebView behavior, the
complete protocol and malicious-preview matrices, and post-download runtime
checks remain unverified for v0.4.0. Physical trackpad momentum, live system
appearance changes, accessibility appearance preferences, and complete native
tray interaction retain the gaps recorded in earlier feature QA.

Builds remain ad-hoc signed and are not notarized.
