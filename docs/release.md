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
11. Publish with `tauri-apps/tauri-action@v0`.

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
- macOS packaging can fail because of platform signing, target, or Tauri
  dependency issues.
- Frontend checks do not validate clipboard behavior; manual desktop QA is
  still required before tagging.
