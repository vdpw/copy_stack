# Release Workflow

## Trigger And Native Targets

`.github/workflows/release.yml` runs for pushed `v*` tags.

```bash
git tag v0.1.0
git push origin v0.1.0
```

The matrix builds each target on matching native hardware:

| Target                 | GitHub runner    |
| ---------------------- | ---------------- |
| `aarch64-apple-darwin` | `macos-15`       |
| `x86_64-apple-darwin`  | `macos-15-intel` |

Normal branch/PR CI also runs its complete automated verification matrix on
both native runners. Native compilation and unit tests improve architecture
coverage; they do not replace the manual Apple Silicon/Intel runtime evidence
required below.

## Automated Release Gate

Each release job:

1. checks out the repository;
2. installs pnpm 10.33.0, Node LTS, Rust, and the matching target;
3. resolves and updates the published `copy_event_listener` package;
4. installs the frozen pnpm lockfile;
5. runs frontend type-check, lint, and tests;
6. runs `pnpm security-check`;
7. checks Rust formatting and runs Rust tests;
8. invokes `tauri-apps/tauri-action@v0`, whose configured build runs the
   frontend production build and packages the matching native target;
9. publishes generated release notes and artifacts.

The checked-in manifest already uses:

```toml
copy_event_listener = "0.1.2"
```

Local development and release builds do not require a sibling
`copy_event_listener` checkout.

The security gate checks strict production CSP, empty static asset scope,
prototype freezing, per-window capabilities, absence of opener permissions,
audited dependency floors, and both native CI/release runner entries.

## Ad-Hoc Signing

The project is not currently enrolled for Apple Developer signing/notarization.
Jobs set:

```yaml
APPLE_SIGNING_IDENTITY: "-"
```

This creates an ad-hoc signature but does not establish a verified developer or
notarize the app. Gatekeeper may require Right-click → Open or
System Settings → Privacy & Security → Open Anyway.

For a trusted downloaded artifact that still reports damage after being moved
to `/Applications`, verify the signature before considering quarantine removal:

```bash
codesign --verify --deep --strict --verbose=4 "/Applications/Copy Stack.app"
xattr -dr com.apple.quarantine "/Applications/Copy Stack.app"
open "/Applications/Copy Stack.app"
```

## Required Pre-Tag Evidence

Run the automated gate in `docs/security-release-checklist.md`, the relevant
performance harness cases in `docs/performance.md`, and the full manual desktop
matrix.

At minimum, record native Apple Silicon and Intel evidence for:

- clean install and existing-database migration, including a second startup
  with no metadata rebuild;
- `0700` directory and `0600` database/sidecar/mirror permissions;
- manual launch, launch at login, app relocation, and duplicate launch;
- protocol skip combinations, source/remote badges, and canonical restore from
  both entry points;
- resource rejection/degradation and malicious HTML;
- 1000-row paging/lazy detail and menu bar behavior with all items plus
  configured smaller limits;
- offline use and unwritable/unsafe private paths;
- slow/failing JSONL destination, last-snapshot integrity, and bounded exit.

The checklist file is a test plan, not proof that these scenarios have already
passed. Attach dated, redacted results to the release record. If one
architecture cannot be exercised, keep the release blocked or obtain and
document an explicit release exception.

## Versions And Prereleases

Check version consistency in:

- `package.json`;
- `src-tauri/Cargo.toml`;
- `src-tauri/tauri.conf.json`.

Tags containing `-alpha`, `-beta`, or `-rc` are published as prereleases.
Others are normal releases. The workflow publishes immediately
(`releaseDraft: false`) and generates release notes.

## Post-Publish Verification

Download each produced artifact on matching native hardware rather than relying
only on the build job. Verify installation/approval, app launch, one synthetic
capture and restore, menu bar operation, single-instance activation, and
launch-at-login state.

## Release Risks

- Published listener behavior can change when its pinned version changes.
- Ad-hoc builds are not notarized and can require explicit Gatekeeper approval.
- Unit tests do not exercise real NSPasteboard, LaunchAgent state, app movement,
  WebView security behavior, or filesystem fault timing.
- Cross-compilation or native compilation alone is not runtime verification.
- Clipboard databases, mirrors, paths, source identifiers, and bodies must not
  appear in release logs or attachments; use synthetic redacted evidence.
