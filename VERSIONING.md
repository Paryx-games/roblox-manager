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

The root `Cargo.toml` is the version source of truth. Both Rust crates inherit it. Synchronise the required version mirrors in `ram_ui/src-tauri/tauri.conf.json` and `ram_ui/package.json` when preparing a release; do not introduce independent version constants.

## Publishing a release

Releases are built and published by `.github/workflows/release.yml`, which runs on pushes of tags matching `v*`, and can also be re-run manually against an existing tag (see [If GitHub is being stubborn](#if-github-is-being-stubborn)). Nothing publishes on a normal push to `main` or any other branch.

1. Bump the root `Cargo.toml` version and synchronise `ram_ui/src-tauri/tauri.conf.json` and `ram_ui/package.json`.

2. Rename `## Unreleased` to `## vX.Y.Z` in `CHANGELOG.md`, preserving its entries. If there is no unreleased section, add the release heading above the previous release.

   > [!WARNING]
   >
   > The release workflow reads this section directly and **fails the release** if it can't find a `## vX.Y.Z` heading matching the tag exactly.

3. If dependencies changed, synchronise and review both lockfiles:

   ```powershell
   cargo build
   git diff Cargo.lock
   pnpm --dir ram_ui install
   git diff ram_ui/pnpm-lock.yaml
   ```

   Include changed lockfiles in the release commit. CI installs with `--frozen-lockfile` and builds with `--locked`. Run the verification sequence in [CONTRIBUTING.md](CONTRIBUTING.md), then smoke-test `pnpm --dir ram_ui tauri build --ci --bundles nsis -- --locked`.

4. Commit those changes:

   ```powershell
   git add Cargo.toml Cargo.lock CHANGELOG.md ram_ui/src-tauri/tauri.conf.json ram_ui/package.json ram_ui/pnpm-lock.yaml
   git commit -m "chore(release): bump version to vX.Y.Z"
   ```

5. Tag the commit and push the tag:

   ```powershell
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

   Pushing the tag is what triggers the workflow.

6. The Windows workflow installs Node 22/pnpm 11 dependencies, builds the embedded frontend and NSIS installer, and publishes `roblox-manager-vX.Y.Z-windows-x64.exe`, `roblox-manager-vX.Y.Z-windows-x64-setup.exe` and `SHA256SUMS.txt`. Notes include the maintained changelog, GitHub-generated notes, download guidance and a VirusTotal report for the direct executable. The workflow requires its `VIRUSTOTAL_API_KEY` secret. Local output remains under `target/release/`; GitHub renaming happens in the release job.

## If GitHub is being stubborn

For a transient runner or service failure with a correct tagged commit, use **Actions > Release > Re-run all jobs**. Alternatively, **Run workflow** accepts an existing tag through `workflow_dispatch`.

Both checkout steps explicitly use that tag. A fix pushed only to `main` does not change the source, lockfiles or changelog built for an existing tag. A manual retry is not a way to include untagged changes.

If the tagged source needs fixing, commit and verify the fix, then publish a new version/tag. Only consider moving an unpublished, unused tag after coordination. Never move a tag that users have downloaded or automation depends on. A retry can replace release assets, so do not use it to silently change an already distributed version.

For custom NSIS template maintenance and the manual installer test matrix, see [Releasing RM](docs/developers/releasing.md).
