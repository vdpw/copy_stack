# v0.3.0 release candidate

Prepared on 2026-09-18 from main commit
`ca6707e61a32cf836a31353ebe1c2e8440b8f686` (merged PR #41).
This release changes the application version to `0.3.0` consistently in
the frontend manifest, Rust manifest and lockfile, and Tauri configuration.

## Changes since v0.2.0

- Pin clipboard items in History and search results. Pinned items appear first
  in History and the menu bar and survive clear-history and retention cleanup.
- Choose System, Light, or Dark appearance, with persisted preferences and
  native window appearance synchronization.
- Use categorized settings with a macOS-style sidebar, compact controls, theme
  previews, and contextual storage help.
- Keep the window background fixed while History and Settings scroll within
  their content areas, preserving search, keyboard navigation, and pagination.
- Accept the pin-aware pagination cursor throughout the command and database
  layers so History continues loading after the first 50 records.

## Local automated and artifact evidence

Environment: Mac15,7, arm64, macOS 27.0 (26A428), Rust 1.93.1, pnpm 10.33.0,
AC power. Foreground workload was not controlled; timings are observations.

- Frozen-lockfile frontend install, security guardrail, type-check, lint, and
  production frontend build passed.
- Frontend: 70 tests passed across 18 files.
- Rust format/check passed; 166 tests passed with the performance test ignored
  by the ordinary suite.
- The ignored release performance matrix then passed separately for text and
  mixed fixtures at 100 and 1000 items, including complete pagination, tray
  counts, retention, and JSONL row accounting.
- An ad-hoc signed arm64 release `.app` and `.dmg` were built with
  `APPLE_SIGNING_IDENTITY=- pnpm desktop:build`. The bundle reports `0.3.0`.
  Strict deep signature verification and `hdiutil verify` passed.
- Local `Copy Stack_0.3.0_aarch64.dmg` SHA-256:
  `8ce1fe335a5d0dd200197f9396beac7ee1a8a172becef1b3f135c3cf61fa1372`.
  GitHub builds produce separate artifacts and need their own verification.

| Fixture | First page | Complete page walk | Tray query | Byte cleanup |
| --- | ---: | ---: | ---: | ---: |
| text / 100 | 0.148 ms | 0.111 ms | 0.055 ms | 2.497 ms |
| text / 1000 | 0.394 ms | 7.101 ms | 0.295 ms | 52.793 ms |
| mixed / 100 | 0.092 ms | 0.087 ms | 0.056 ms | 1.933 ms |
| mixed / 1000 | 0.203 ms | 3.627 ms | 0.286 ms | 47.221 ms |

The 1000-item byte-cleanup measurements exceed the historical 15 ms review
signal. They are close to the 47–51 ms measurements documented for v0.2.0,
which attributed the cost to FTS delete-trigger scans. This run does not claim
that the existing bulk-cleanup limitation has been fixed.

## Desktop coverage and release decision

Feature-level Apple Silicon desktop observations are recorded in
[Pin and macOS UI QA](pin-and-macos-ui.md) and
[Appearance and Settings QA](appearance-and-settings.md). They cover synthetic
pin/clear behavior, theme persistence, settings navigation, scrolling,
keyboard focus, and loading 65 records through the application.

These debug-app observations do not complete the packaged desktop matrix in
[the release checklist](../security-release-checklist.md). Native Intel
clipboard/window behavior, login/logout/reboot, application relocation,
offline WebView behavior, the complete protocol and malicious-preview matrix,
and post-download smoke tests remain unverified. Physical trackpad momentum,
system appearance transitions, accessibility appearance preferences, and the
full native tray interaction matrix also retain the gaps noted in feature QA.

The v0.1.0, v0.1.1, and v0.2.0 release exceptions are version-specific. No
v0.3.0 exception approval is recorded here. Per the existing release checklist,
the release tag remains pending completion of the missing matrix or an
explicit release-owner exception. Builds remain ad-hoc signed and not notarized.
