# Release Workflow

## Release Trigger

Releases are handled by `.github/workflows/release.yml`.

The workflow runs on pushed tags matching:

```text
v*
```

Example:

```bash
git tag v0.1.0
git push origin v0.1.0
```

## Build Targets

The current workflow builds macOS packages for:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

It runs on `macos-latest`.

## Workflow Steps

1. Check out the repository.
2. Install pnpm 10.33.0.
3. Install Node.js LTS with pnpm cache.
4. Install stable Rust with both macOS targets.
5. Cache Rust dependencies.
6. Replace the local `copy_event_listener` path dependency with the published
   crate `copy_event_listener = "0.1.2"`.
7. Run `cargo update` for `copy_event_listener`.
8. Install frontend dependencies with `pnpm install --frozen-lockfile`.
9. Run `pnpm type-check`.
10. Run `pnpm lint`.
11. Publish ad-hoc signed macOS artifacts with `tauri-apps/tauri-action@v0`.

## Ad-Hoc Signed macOS Releases

The project currently publishes ad-hoc signed macOS builds because it is not
enrolled in the Apple Developer Program. The release workflow sets:

```yaml
APPLE_SIGNING_IDENTITY: "-"
```

This does not notarize the app and does not identify a verified Apple
developer. It only gives the app bundle a local ad-hoc code signature, which is
especially important for Apple Silicon downloads.

Ad-hoc signed apps downloaded from GitHub can still trigger macOS Gatekeeper
warnings. Users who trust the release should usually be able to approve it with
one of these macOS flows:

1. Right-click `Copy Stack.app`, choose `Open`, then confirm.
2. Open `System Settings > Privacy & Security`, then choose `Open Anyway`.

If macOS still shows:

```text
"Copy Stack.app" is damaged and can't be opened. You should move it to the Trash.
```

then remove the quarantine attribute after dragging the app into
`/Applications`:

```bash
xattr -dr com.apple.quarantine "/Applications/Copy Stack.app"
open "/Applications/Copy Stack.app"
```

Users can also build from source with the development tooling instead of using a
downloaded artifact.

## Local Dependency Replacement

Development uses:

```toml
copy_event_listener = { path = "../../copy_event_listener" }
```

GitHub Actions replaces it with:

```toml
copy_event_listener = "0.1.2"
```

Do not commit the release workflow's temporary replacement unless the project is
intentionally moving away from local path development.

## Version Locations

Check these before a release:

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`

The release name uses the pushed tag:

```text
Copy Stack ${{ github.ref_name }}
```

## Prerelease Tags

The workflow marks a release as prerelease if the tag contains:

- `-alpha`
- `-beta`
- `-rc`

## Release Checklist

Before pushing a release tag:

1. Confirm versions are correct.
2. Run `pnpm type-check`.
3. Run `pnpm lint`.
4. Run `cargo check --manifest-path src-tauri/Cargo.toml`.
5. Run `pnpm desktop:build` locally when platform signing and dependencies
   allow.
6. Validate clipboard capture, restore, tray menu, settings, and retention with
   `pnpm desktop:dev`.
7. Confirm `copy_event_listener = "0.1.2"` is still the intended published crate
   for release builds.
8. After the release uploads, download the `.dmg` on a clean macOS machine and
   verify that the app can be approved through `Privacy & Security > Open
   Anyway`.
9. If `Open Anyway` does not appear, verify the ad-hoc signature and then test
   the quarantine fallback:

   ```bash
   codesign --verify --deep --strict --verbose=4 "/Applications/Copy Stack.app"
   xattr -dr com.apple.quarantine "/Applications/Copy Stack.app"
   open "/Applications/Copy Stack.app"
   ```

## Release Artifacts

`tauri-apps/tauri-action@v0` creates GitHub release artifacts for the configured
targets and icons in `src-tauri/tauri.conf.json`.

The workflow uses:

- `releaseDraft: false`
- `generateReleaseNotes: true`
- `GITHUB_TOKEN` from repository secrets

## Release Risk Areas

- The published `copy_event_listener` crate can differ from the local path
  checkout used during development.
- macOS packaging can fail because of signing, target, or Tauri dependency
  issues.
- Ad-hoc signed releases are not notarized. Gatekeeper can still require user
  approval after browser download, but the app should not look like a malformed
  or damaged bundle.
- Frontend checks do not validate clipboard behavior; manual desktop QA is
  still required before tagging.
