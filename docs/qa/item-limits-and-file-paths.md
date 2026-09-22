# Item limits, pinned deletion, and file paths

Validated on 2026-09-22, Apple Silicon (`arm64`), macOS 27.0,
Rust 1.98.1. All history, file paths, and pasteboard fixtures were synthetic.
Desktop checks used an isolated temporary SQLite database and the development
frontend. The debug executable was also wrapped in a temporary QA app bundle
so native window automation could access it.

## Automated checks

- Frontend: 90 tests, type-check, lint, and production build passed.
- Backend: 183 tests passed; the performance harness remains intentionally
  ignored. Cargo check and format check passed.
- Security configuration guardrails and `git diff --check` passed.

Tests cover whole-MiB validation, encoded-size boundaries, persisted settings,
preservation of existing rows, bounded large-content previews and display
normalization, restore suppression after lowering the limit, deletion-dialog
keyboard/focus/retry behavior, and file/folder detail paths (including native
Finder file-reference URLs).

## Native desktop checks

- Saved 64 MiB in Clipboard settings and confirmed SQLite stored `67108864`.
  The setting survived a restart.
- Captured a 33 MiB text fixture: its event blob was 34,603,094 bytes while its
  stored display remained 1 MiB and list summary remained 512 bytes. History
  showed one new row and successfully restored it to the real pasteboard.
- Lowered the limit to 32 MiB: existing history remained, new 33 MiB fixtures
  were rejected, and History showed the rejection notice without adding rows.
- Restored the existing large row below the new limit without a false
  rejection notice. A subsequent different external oversized copy was still
  rejected. This was repeated after rebuilding the suppression-order fix.
- Requested deletion of a pinned fixture: the confirmation identified the pin
  and permanent deletion, initially focused Cancel, and Escape preserved the
  item and returned focus to its Delete button. Confirmation execution and
  failure retries are covered by automated interaction tests.
- Expanded a mixed file/folder fixture in both themes: full decoded paths
  appeared below names in smaller, secondary-color text; collapsed rows showed
  only names.
- Captured the History and Clipboard settings pages in Light and Dark.
  README references all four 760 × 820 screenshots in `docs/images/`.

This is feature QA on Apple Silicon, not an Intel release qualification or a
guarantee of latency at the 256 MiB ceiling. Larger captures and restores still
process full clipboard bodies; preview limits bound the UI payload separately.
