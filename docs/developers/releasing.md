---
description: Prepare a version and publish an RM release through GitHub Actions.
icon: tag
---

# Releasing RM

## Versioning

RM follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- **MAJOR** — Breaking changes require migration. This includes incompatible configuration or account-store formats. It also includes removing or renaming dependencies of existing setups, scripts, or integrations. Feature removal alone does not require a major release.
- **MINOR** — Backward-compatible features, settings, tabs, capabilities, or feature removals.
- **PATCH** — Bug fixes, wording changes, or UI polish. Do not add capability.

### Pre-releases

RM has no project-wide `0.x` or beta phase. Each version line ships as stable `MAJOR.MINOR.PATCH`.

Pre-release identifiers stage an upcoming version before its plain tag:

- **`-alpha.N`** — An early, unfinished build. Expect breakage.
- **`-beta.N`** — An early test build. It may lack features or contain bugs.
- **`-rc.N`** — A release candidate for final testing. If no issues appear, publish the plain version next.

{% hint style="info" %}
An `-rc.N` version requires an earlier `-alpha.N` or `-beta.N` for that version. Otherwise, publish the plain version directly.
{% endhint %}

Under SemVer, pre-releases sort before their plain release. For example, `v2.0.0-rc.1` is earlier than `v2.0.0`. Tooling should not treat pre-releases as the latest stable version.

### Where the version lives

Set the version only in the root `Cargo.toml`. Both crates inherit it. Do not hardcode versions elsewhere.

## Publishing a release

`.github/workflows/release.yml` runs when you push a tag matching `v*`, and can also be re-run manually against an existing tag (see [If GitHub is being stubborn](#if-github-is-being-stubborn)). Normal pushes to `main` or other branches do not publish releases.

1. Bump the version in the root `Cargo.toml`.
2. Add `## vX.Y.Z` to `CHANGELOG.md`, above the previous release.

<div data-gb-custom-block data-tag="hint" data-style="warning" class="hint hint-warning"><p>The workflow fails if it cannot find a <code>## vX.Y.Z</code> heading matching the tag exactly.</p></div>

1. Sync `Cargo.lock` when dependencies changed:

```powershell
cargo build
git diff Cargo.lock
```

Check that the diff is expected. Include `Cargo.lock` in the release commit. The workflow uses `--locked` and fails with a stale lockfile.

1. Commit the release files:

```powershell
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: bump version to vX.Y.Z"
```

1. Tag the commit and push the tag:

```powershell
git tag vX.Y.Z
git push origin vX.Y.Z
```

Pushing the tag triggers the workflow.

1. The workflow builds the release and publishes it automatically. It renames the executable to `roblox-manager-vX.Y.Z-windows-x64.exe`. It also adds changelog content, GitHub-generated notes, a downloads table, and a SHA256 checksum.

### If GitHub is being stubborn

The workflow has a `workflow_dispatch` trigger that takes the tag to release as an input, so a failed run doesn't require touching the tag at all:

1. Fix whatever broke (stale `Cargo.lock`, a bad changelog heading, etc.) and push the fix to `main` as a normal commit.
2. Open **Actions** → **Release** → **Run workflow**.
3. Enter the existing tag (e.g. `v2.0.0`) and run it.

This runs the workflow fresh, including your fix, and publishes or overwrites the release for that tag without moving or recreating anything.

{% hint style="info" %}
Don't delete and re-push the tag to force a rebuild. GitHub Actions treats a moved tag as suspicious and often refuses to run the workflow against it (or runs it inconsistently), so it isn't a reliable retry path — use `workflow_dispatch` instead.
{% endhint %}

If the tagged commit is already correct and the run just failed transiently (flaky runner, GitHub API hiccup, etc.), you don't even need `workflow_dispatch` — rerun it from **Actions**. Use **Re-run all jobs**.
