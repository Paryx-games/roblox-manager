# Versioning

RM follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- **MAJOR** - breaking changes **or a substantial project-scale release**: an incompatible config or account-store format that requires migration, removing/renaming something existing setups, scripts, or integrations may depend on, **or a release that represents a significant new generation of RM through major architectural work, a substantial rewrite, a large collection of tightly related features, or a material change in the project's scope or direction**. Effort alone is not enough - use MAJOR when the result is substantial enough to represent a genuine milestone in RM's development.

- **MINOR** - new features, added settings, new tabs/capabilities, or removing a feature in a way that doesn't break existing configs. Backward-compatible.

- **PATCH** - bug fixes, wording/UI polish, no new capability.

## Pre-releases

RM has no 0.x or beta phase for the project as a whole - every version line still ships as a stable `MAJOR.MINOR.PATCH`. Pre-release identifiers (`-alpha.N`, `-beta.N`, `-rc.N`) are used to stage testing of an _upcoming_ version before that version's plain tag goes out.

- **`-alpha.N`** (e.g. `v2.0.0-alpha.1`) - an early, unfinished build published well before feature-complete. Expect breakage.

- **`-beta.N`** (e.g. `v2.0.0-beta.1`) - an early build of an upcoming version, published for testing before it's considered feature-complete or stable. Expect bugs.

- **`-rc.N`** ("release candidate", e.g. `v2.0.0-rc.1`) - a build believed ready to ship, published for a final round of testing before the real tag goes out. If no issues turn up, the next tag is the plain version with no suffix.

> [!IMPORTANT]
>
> `-rc.N` can only be used for a version that has already had a `-beta` or `-alpha` pre-release. A version can't jump straight to `-rc.N` without going through beta or alpha testing first - if there's no earlier `-beta`/`-alpha` tag for that version, skip straight to the plain release instead.

> [!NOTE]
>
> Under SemVer, a pre-release version sorts _before_ the plain version it leads up to (`v2.0.0-rc.1` < `v2.0.0`), and tooling generally should not treat a pre-release as the "latest" stable version.

## Where the version lives

The version lives once in the root `Cargo.toml` and both crates inherit it - do not hardcode a version anywhere else.

## Publishing a release

Releases are built and published by `.github/workflows/release.yml`, which runs on pushes of tags matching `v*`, and can also be re-run manually against an existing tag (see [If GitHub is being stubborn](#if-github-is-being-stubborn)). Nothing publishes on a normal push to `main` or any other branch.

1. Bump the version in the root `Cargo.toml`.

2. Add a `## vX.Y.Z` entry to `CHANGELOG.md` for the new version, above the previous entry.

   > [!WARNING]
   >
   > The release workflow reads this section directly and **fails the release** if it can't find a `## vX.Y.Z` heading matching the tag exactly.

3. If `Cargo.lock` is out of sync with `Cargo.toml` (new/updated deps), sync it before committing:

   ```powershell
   cargo build
   git diff Cargo.lock
   ```

   Check the diff looks sane, then include `Cargo.lock` in the commit below. The workflow builds with `--locked`, which fails outright if the lockfile is stale - see [If GitHub is being stubborn](#if-github-is-being-stubborn) if this bites you after the tag's already pushed.

4. Commit those changes:

   ```powershell
   git add Cargo.toml Cargo.lock CHANGELOG.md
   git commit -m "chore: bump version to vX.Y.Z"
   ```

5. Tag the commit and push the tag:

   ```powershell
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

   Pushing the tag is what triggers the workflow.

6. The workflow builds and renames the executable to `roblox-manager-vX.Y.Z-windows-x64.exe`, generates release notes from the changelog entry plus GitHub's auto-generated notes, adds a downloads table and SHA256 checksum, and publishes the GitHub release automatically. No manual steps after pushing the tag are needed unless the run fails.

## If GitHub is being stubborn

The release workflow has a `workflow_dispatch` trigger that takes the tag to release as an input, so a failed run doesn't require touching the tag at all:

1. Fix whatever broke (stale `Cargo.lock`, a bad changelog heading, etc.) and push the fix to `main` as a normal commit.
2. Open the **Actions** tab -> **Release** -> **Run workflow**.
3. Enter the existing tag (e.g. `v2.0.0`) and run it.

This runs the workflow fresh - including your fix - and publishes/overwrites the release for that tag, without moving or recreating anything.

> [!NOTE]
>
> Don't delete and re-push the tag to force a rebuild. GitHub Actions treats a moved tag as suspicious and will often refuse to run the workflow against it (or run it inconsistently), so it's not a reliable retry path - use `workflow_dispatch` instead.

If the commit the tag already points to is fine and the run just failed transiently (flaky runner, GitHub API hiccup on the notes-generation step, etc.), you don't even need `workflow_dispatch` - just open the failed run under the **Actions** tab and use **Re-run all jobs**.
