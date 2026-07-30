# Security and Release Verification

This document is a test plan. Its presence and a green automated build are not
evidence that the manual rows below have passed.

Run the automated gate before every release:

```bash
pnpm install --frozen-lockfile
pnpm security-check
pnpm type-check
pnpm lint
pnpm test
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

The release workflow treats any failure as blocking. All fixtures must be
synthetic and must not include real clipboard contents, user paths, source
identifiers, credentials, or exported history.

CI and release gates execute natively on `macos-15` Apple Silicon and
`macos-15-intel`. That dual-architecture coverage validates dependency
resolution, builds, security guardrails, and automated tests on both
architectures. It does not exercise real NSPasteboard workflows, WebView
navigation, login items, application relocation, filesystem fault timing, or
window focus; record those rows manually.

## Desktop QA matrix

Record the app version, macOS version, architecture, tester, timestamp, and
redacted evidence for each run. Screenshots and logs must use synthetic
clipboard values.

| Scenario                       | Platforms               | Expected result                                                                                                                                                      | Evidence to record                                     |
| ------------------------------ | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ |
| Clean install                  | Apple Silicon and Intel | Data directory is `0700`; database and any created WAL/SHM files are `0600`; launch-at-login is off                                                                  | Version, platform, permission modes                    |
| Existing database upgrade      | Apple Silicon and Intel | Migration is atomic, preserves ordering and settings, and a second launch performs no rebuild                                                                        | Schema versions, row counts, timing summary            |
| App moved after install        | Apple Silicon and Intel | Manual launch works; launch-at-login can be disabled and re-enabled using the new location                                                                           | Launch result and verified OS login-item state         |
| Launch at login                | Apple Silicon and Intel | The app starts quietly with no main window and exactly one listener                                                                                                  | Process count and window state                         |
| Manual launch                  | Apple Silicon and Intel | The primary window appears and receives clipboard updates                                                                                                            | Window and synthetic capture result                    |
| Concurrent duplicate launch    | Apple Silicon and Intel | Only one database/listener remains; the existing main window is shown and focused                                                                                    | Process/listener count and focus result                |
| Offline launch and use         | Apple Silicon and Intel | History, settings, restore, and previews work without external network access                                                                                        | Network-disabled run result                            |
| Unwritable or unsafe data path | Apple Silicon and Intel | Startup fails safely with a user-actionable error; no public fallback file is created                                                                                | Error code and filesystem result                       |
| JSONL slow/failing destination | Apple Silicon and Intel | Database commits continue; the last valid snapshot remains complete; exit flush is bounded                                                                           | Commit result and snapshot checksum                    |
| Resource-limit handling        | Apple Silicon and Intel | Oversized rich content safely degrades to bounded plain text when eligible; otherwise it is not persisted or sent over IPC and a localized notice appears            | Fixture type, size bucket, safe result code, row count |
| Malicious HTML preview         | Apple Silicon and Intel | No script, navigation, external request, form, or unsafe URL runs; safe formatting remains                                                                           | Sanitizer test result and network trace                |
| NSPasteboard protocol matrix   | Apple Silicon and Intel | Transient, autogenerated, concealed, and 1Password-marked synthetic clips never appear; source and remote metadata follow policy                                     | Marker combination and downstream absence              |
| Restore protocol metadata      | Apple Silicon and Intel | Restored pasteboard contains exactly one source marker and the remote marker only when applicable                                                                    | Synthetic pasteboard type list                         |
| Tray with 1, 20, and 1000 rows | Apple Silicon and Intel | The default tray shows all retained rows; configured limits show exactly the newest requested rows, each row copies directly, and rebuilding never reads rich detail | Menu count, click result, and timing summary           |
| Paged history with 1000 rows   | Apple Silicon and Intel | First load transfers one page; details load only on expansion; scroll and expansion state remain stable                                                              | Page/detail request counts and timing                  |

## Release evidence

Attach the completed matrix to the release record. If either architecture
cannot be exercised, keep the release blocked or explicitly document and
approve that exception; a successful cross-compilation alone is not runtime
verification.

## Local evidence: 2026-07-28

This evidence was collected against version `0.1.0` on an Apple M3 Pro MacBook
Pro (`arm64`, macOS 26.5.2). All clipboard fixtures used synthetic text and
markers. The app ran with `COPY_STACK_QA_DATA_DIR` pointing to a private
directory under macOS's per-user temporary area; the real Copy Stack database
was not opened. The system pasteboard was backed up to a `0600` file and
restored after the sequence.

| Check                                | Result and redacted evidence                                                                                                                                                                                                                                                    |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Automated gate                       | Frontend format, type-check, lint, 20 tests, production build, security guardrail, and production dependency audit passed. Rust format/check and 129 tests passed; the manual timing harness remained intentionally ignored.                                                    |
| Performance                          | Fixture-v3 100/1000 text/mixed release matrix passed. The database-backed JSONL observation produced exactly 1000 records for each 1000-row case; timings and before/after results are in `docs/performance.md`.                                                                |
| Apple Silicon artifact               | Ad-hoc signed `.app` and verified read-only DMG were produced. The mounted executable is a thin `arm64` Mach-O and `codesign --verify --deep --strict` passed.                                                                                                                  |
| Intel artifact and automated runtime | Thin `x86_64` binary, ad-hoc signed `.app`, and verified read-only DMG were produced. The full Rust suite passed under Rosetta: 129 passed, one timing harness ignored. This is not native Intel hardware evidence.                                                             |
| Private storage and defaults         | Isolated data directory was `0700`; SQLite database was `0600`; schema version was 2. Item limit was 100, byte limit 256 MiB, compact mode and restore-to-top were off, and no Copy Stack LaunchAgent existed before user interaction.                                          |
| Real pasteboard protocol capture     | Plain, explicit-empty source, valid synthetic source, and remote-marker clips produced four rows with the expected nullable/empty/source/remote metadata. Universal transient, autogenerated, concealed, and synthetic 1Password-marked writes each left the row count at four. |
| Single instance                      | Normal and malformed-argument second launches exited successfully while the original debug listener and database owner continued running. The malformed launch reported only a stable startup-options message.                                                                  |
| Filesystem/mirror fault coverage     | Automated tests used SQLite-created rollback journal, WAL, and SHM files and verified/repaired `0600` modes. Mirror pre/post-commit injection, stale-generation interleaving, atomic replacement, symlink/hardlink rejection, and bounded shutdown passed.                      |

The Mac was locked during the final 2026-07-28 visual pass, so Computer Use
could not inspect or operate the window then. A read-only visual pass on
2026-07-29 opened Settings from an isolated debug bundle and confirmed the
authoritative launch-at-login state rendered as off. It did not change that
persistent OS preference, so enable/disable read-back remains unclaimed.
History restore clicks and focus appearance also remain unclaimed. Native Intel
clipboard, login/logout/reboot, app-relocation, offline WebView,
TextExpander/Universal Clipboard, and packaged window QA remain release-blocking
manual rows unless a release owner explicitly approves an exception.
