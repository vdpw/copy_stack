# Storage Location QA

Local verification on 2026-09-22, ClipEcho 0.4.0, macOS 27.0 arm64.
All storage fixtures were isolated under a private temporary QA root through
`COPY_STACK_QA_DATA_DIR`. The ordinary application database was not moved.

## Automated checks

- Frontend type-check, lint, formatting, production build, security guardrail:
  passed.
- Frontend tests: 101 passed, including directory-change success, rollback,
  cancellation, duplicate-action prevention, and stale-response handling.
- Rust tests: 198 passed; the existing timing harness remained ignored.
- Rust formatting and compilation: passed.
- Debug macOS app bundle: built successfully.

Rust scenarios cover WAL-backed relocation, history/pins/search/settings
preservation, restart lookup, conflicts with database and sidecar names,
unwritable destinations, real source-deletion permission failures, injected
configuration/cleanup failures, interrupted-move recovery, missing configured
databases, changed file identities, and mirror reader exclusion/retargeting.

## Native UI and filesystem checks

- Clipboard settings displayed the active storage directory and Change button.
- Change opened the native directory-only chooser.
- Selecting an empty owned folder moved the database, updated the displayed
  path and bootstrap configuration, and removed the original database.
  SQLite `quick_check` returned `ok` on the destination.
- Selecting a folder containing a synthetic `clipecho.db` sentinel displayed
  the localized filename-conflict failure. The active path stayed unchanged,
  and the sentinel was not overwritten.
- Selecting a folder with owner mode `0500` displayed the permission failure.
  The active path/configuration stayed unchanged and no database was created
  in the denied folder.
- Restarted the final debug bundle with the same QA bootstrap root; Clipboard
  settings still displayed the migrated directory and the database initialized
  normally.

Native Intel and a physical cross-volume move were not exercised in this run.
