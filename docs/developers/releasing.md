---
description: Prepare a version and publish an RM release through GitHub Actions.
icon: tag
---

# Releasing RM

Releases are built by `.github/workflows/release.yml` when a `v*` tag is pushed. The workflow publishes the Windows executable and release notes automatically.

## Prepare the release

1. Update the version in the root `Cargo.toml`. The two crates inherit it.
2. Add a matching `## vX.Y.Z` section at the top of `CHANGELOG.md`.
3. Run formatting, checks, tests, and clippy.
4. Commit the version and changelog together.

The changelog heading must match the tag exactly or the release workflow stops before publishing.

## Tag and publish

```powershell
git tag vX.Y.Z
git push origin vX.Y.Z
```

The Windows job builds `target/release/ram_ui.exe`. The release job adds the changelog section, GitHub-generated notes, a SHA256 checksum, and the executable to the GitHub release.

Normal pushes to `main` do not publish a release. If a release run fails, inspect the workflow logs, fix the source issue, and publish a new version tag according to the repository's release policy.

## Versioning rules

Use PATCH for fixes and documentation polish, MINOR for backward-compatible features, and MAJOR for breaking changes such as incompatible account-store or configuration formats. Pre-release tags are only appropriate when that release path has been intentionally introduced and documented.