---
description: Prepare a version and publish an RM release through GitHub Actions.
icon: tag
---

# Releasing RM

## Versioning

RM follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

* **MAJOR** — Breaking changes require migration. This includes incompatible configuration or account-store formats. It also includes removing or renaming dependencies of existing setups, scripts, or integrations. Feature removal alone does not require a major release.
* **MINOR** — Backward-compatible features, settings, tabs, capabilities, or feature removals.
* **PATCH** — Bug fixes, wording changes, or UI polish. Do not add capability.

### Pre-releases

RM has no project-wide `0.x` or beta phase. Each version line ships as stable `MAJOR.MINOR.PATCH`.

Pre-release identifiers stage an upcoming version before its plain tag:

* **`-alpha.N`** — An early, unfinished build. Expect breakage.
* **`-beta.N`** — An early test build. It may lack features or contain bugs.
* **`-rc.N`** — A release candidate for final testing. If no issues appear, publish the plain version next.

{% hint style="info" %}
An `-rc.N` version requires an earlier `-alpha.N` or `-beta.N` for that version. Otherwise, publish the plain version directly.
{% endhint %}

Under SemVer, pre-releases sort before their plain release. For example, `v2.0.0-rc.1` is earlier than `v2.0.0`. Tooling should not treat pre-releases as the latest stable version.

### Where the version lives

Set the version only in the root `Cargo.toml`. Both crates inherit it. Do not hardcode versions elsewhere.

## Publishing a release

`.github/workflows/release.yml` runs only when you push a tag matching `v*`. Normal pushes to `main` or other branches do not publish releases.

1. Bump the version in the root `Cargo.toml`.
2.  Add `## vX.Y.Z` to `CHANGELOG.md`, above the previous release.

    <div data-gb-custom-block data-tag="hint" data-style="warning" class="hint hint-warning"><p>The workflow fails if it cannot find a <code>## vX.Y.Z</code> heading matching the tag exactly.</p></div>
3. Sync `Cargo.lock` when dependencies changed:

```powershell
cargo build
git diff Cargo.lock
```

Check that the diff is expected. Include `Cargo.lock` in the release commit. The workflow uses `--locked` and fails with a stale lockfile.

4. Commit the release files:

```powershell
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: bump version to vX.Y.Z"
```

5. Tag the commit and push the tag:

```powershell
git tag vX.Y.Z
git push origin vX.Y.Z
```

Pushing the tag triggers the workflow.

6. The workflow builds the release and publishes it automatically. It renames the executable to `roblox-manager-vX.Y.Z-windows-x64.exe`. It also adds changelog content, GitHub-generated notes, a downloads table, and a SHA256 checksum.

### If GitHub is being stubborn

The workflow has no `workflow_dispatch` trigger. It only runs on tag pushes.

If a fix exists in a newer commit, move the tag:

```powershell
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag vX.Y.Z
git push origin vX.Y.Z
```

This recreates the tag at the latest commit. Pushing it triggers the workflow again.

{% hint style="warning" %}
Move a tag only before anyone relies on it. Use a new version if users downloaded the executable or automation pins the tag.
{% endhint %}

If the tagged commit is correct, rerun it from **Actions**. Use **Re-run all jobs** for transient failures.
