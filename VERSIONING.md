# Versioning

RM follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- **MAJOR** — breaking changes: an incompatible config or account-store format that requires migration, or removing/renaming something existing setups, scripts, or integrations may depend on. Most feature removals don't need this on their own; reserve MAJOR for changes that actually break something for existing users.
- **MINOR** — new features, added settings, new tabs/capabilities, or removing a feature in a way that doesn't break existing configs. Backward-compatible.
- **PATCH** — bug fixes, wording/UI polish, no new capability.

## Pre-releases

RM has no 0.x or beta phase for the project as a whole - every version line still ships as a stable `MAJOR.MINOR.PATCH`. Pre-release identifiers (`-alpha.N`, `-beta.N`, `-rc.N`) are used to stage testing of an _upcoming_ version before that version's plain tag goes out.

- **`-alpha.N`** (e.g. `v2.0.0-alpha.1`) — an early, unfinished build published well before feature-complete. Expect breakage.
- **`-beta.N`** (e.g. `v2.0.0-beta.1`) — an early build of an upcoming version, published for testing before it's considered feature-complete or stable. Expect bugs.
- **`-rc.N`** ("release candidate", e.g. `v2.0.0-rc.1`) — a build believed ready to ship, published for a final round of testing before the real tag goes out. If no issues turn up, the next tag is the plain version with no suffix.

> [!IMPORTANT]
> `-rc.N` can only be used for a version that has already had a `-beta` or `-alpha` pre-release. A version can't jump straight to `-rc.N` without going through beta or alpha testing first — if there's no earlier `-beta`/`-alpha` tag for that version, skip straight to the plain release instead.

> [!NOTE]
> Under SemVer, a pre-release version sorts _before_ the plain version it leads up to (`v2.0.0-rc.1` < `v2.0.0`), and tooling generally should not treat a pre-release as the "latest" stable version.

## Where the version lives

The version lives once in the root `Cargo.toml` and both crates inherit it — do not hardcode a version anywhere else.

## Publishing a release

Releases are built and published by `.github/workflows/release.yml`, which only runs on pushes of tags matching `v*`. Nothing publishes on a normal push to `main` or any other branch.

1. Bump the version in the root `Cargo.toml`.
2. Add a `## vX.Y.Z` entry to `CHANGELOG.md` for the new version, above the previous entry.

   > [!WARNING]
   > The release workflow reads this section directly and **fails the release** if it can't find a `## vX.Y.Z` heading matching the tag exactly.

3. If `Cargo.lock` is out of sync with `Cargo.toml` (new/updated deps), sync it before committing:

   ```powershell
   cargo build
   git diff Cargo.lock
   ```

   Check the diff looks sane, then include `Cargo.lock` in the commit below. The workflow builds with `--locked`, which fails outright if the lockfile is stale — see [If GitHub is being stubborn](#if-github-is-being-stubborn) if this bites you after the tag's already pushed.

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

The release workflow currently has **no `workflow_dispatch` trigger** — it only runs on a tag push. So if a run fails or you need to rebuild the same version (e.g. after fixing a stale `Cargo.lock`), re-running the job from the Actions tab won't help if the fix lives in a newer commit than the one the tag points to. In that case, move the tag instead:

```powershell
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag vX.Y.Z
git push origin vX.Y.Z
```

This deletes the tag locally and on GitHub, recreates it pointing at your latest commit, and pushes it — which re-triggers the workflow against the fixed commit.

> [!WARNING]
> Only do this for a tag that hasn't been relied on yet (no one's grabbed the exe, no downstream automation pinned to it). If the release is already public and in use, bump to a new version instead of moving the tag out from under people.

If the commit the tag already points to is fine and the run just failed transiently (flaky runner, GitHub API hiccup on the notes-generation step, etc.), you can re-run that exact commit without moving anything: open the failed run under the **Actions** tab and use **Re-run all jobs**.
