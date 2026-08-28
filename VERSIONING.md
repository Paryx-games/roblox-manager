# Versioning

RM follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- **MAJOR** — breaking changes: an incompatible config or account-store format that requires migration, or removing/renaming something existing setups, scripts, or integrations may depend on. Most feature removals don't need this on their own; reserve MAJOR for changes that actually break something for existing users.
- **MINOR** — new features, added settings, new tabs/capabilities, or removing a feature in a way that doesn't break existing configs. Backward-compatible.
- **PATCH** — bug fixes, wording/UI polish, no new capability.

## Pre-releases

RM has no 0.x or beta phase. Every release is a stable `MAJOR.MINOR.PATCH` version.

Pre-release identifiers (`-beta.N`, `-rc.N`) are not currently used. If they're introduced in the future:

- **`-beta.N`** (e.g. `v2.0.0-beta.1`) — an early build of an upcoming version, published for testing before it's considered feature-complete or stable. Expect bugs.
- **`-alpha.N`** (e.g. `v2.0.0-alpha.1`) — an early, unfinished build published well before feature-complete. Expect breakage.
- **`-rc.N`** ("release candidate", e.g. `v2.0.0-rc.1`) — a build believed ready to ship, published for a final round of testing before the real tag goes out. If no issues turn up, the next tag is the plain version with no suffix.

> [!IMPORTANT]
> `-rc.N` can only be used for a version that has already had a `-beta` or `-alpha` pre-release. A version can't jump straight to `-rc.N` without going through beta or alpha testing first — if there's no earlier `-beta`/`-alpha` tag for that version, skip straight to the plain release instead.

> [!NOTE]
> Under SemVer, a pre-release version sorts *before* the plain version it leads up to (`v2.0.0-rc.1` < `v2.0.0`), and tooling generally should not treat a pre-release as the "latest" stable version.

## Where the version lives

The version lives once in the root `Cargo.toml` and both crates inherit it — do not hardcode a version anywhere else.

## Publishing a release

Releases are built and published by `.github/workflows/release.yml`, which only runs on tags matching `v*`.

1. Bump the version in the root `Cargo.toml`.
2. Add a `## vX.Y.Z` entry to `CHANGELOG.md` for the new version, above the previous entry.

   > [!WARNING]
   > The release workflow reads this section directly and **fails the release** if it can't find a `## vX.Y.Z` heading matching the tag exactly.

3. Commit those changes.
4. Tag the commit and push the tag:

   ```
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

   > [!TIP]
   > Pushing the tag is what triggers the workflow — nothing publishes on a normal push to `main`.

5. The workflow builds `ram_ui.exe`, generates release notes from the changelog entry plus GitHub's auto-generated notes, adds a SHA256 checksum, and publishes the GitHub release automatically. No manual steps after pushing the tag are needed unless the run fails.

> [!TIP]
> If a release run gets stuck, it can be re-triggered manually via `workflow_dispatch` from the Actions tab.
