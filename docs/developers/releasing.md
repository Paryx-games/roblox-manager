---
description: Prepare a version and publish an RM release through GitHub Actions.
icon: tag
---

# Releasing RM

## Versioning

RM follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- **MAJOR** - Breaking changes requiring migration, or a substantial new generation of RM through significant architectural work or a material scope change. Effort alone does not justify a major version. Follow [VERSIONING.md](https://github.com/Paryx-games/roblox-manager/blob/main/VERSIONING.md) for the complete policy.
- **MINOR** - Backward-compatible features, settings, tabs, capabilities, or feature removals.
- **PATCH** - Bug fixes, wording changes, or UI polish. Do not add capability.

### Pre-releases

RM has no project-wide `0.x` or beta phase. Each version line ships as stable `MAJOR.MINOR.PATCH`.

Pre-release identifiers stage an upcoming version before its plain tag:

- **`-alpha.N`** - An early, unfinished build. Expect breakage.
- **`-beta.N`** - An early test build. It may lack features or contain bugs.
- **`-rc.N`** - A release candidate for final testing. If no issues appear, publish the plain version next.

{% hint style="info" %}
An `-rc.N` version requires an earlier `-alpha.N` or `-beta.N` for that version. Otherwise, publish the plain version directly.
{% endhint %}

Under SemVer, pre-releases sort before their plain release. For example, `v2.0.0-rc.1` is earlier than `v2.0.0`. Tooling should not treat pre-releases as the latest stable version.

### Where the version lives

The root `Cargo.toml` `[workspace.package].version` is the canonical version. Both Rust crates inherit it, and Tauri derives it from Cargo with no `version` override in `tauri.conf.json`. Only `ram_ui/package.json` needs an automatically synchronised mirror.

Run `bump-version.bat` from the repository root for an interactive bump, or `pnpm version:set 2.0.0-beta.3` for automation. The batch script exits after the result; use `bump-version.bat --pause` when you want it to wait for a key before closing. Run `pnpm version:sync` after a manual Cargo edit, and `pnpm version:check` to verify consistency without modifying files. These commands also work with `pnpm --dir ram_ui`.

The tools validate the project version format and Windows component limits, require an earlier alpha/beta tag for rc versions, update Cargo.lock through Cargo, and verify the frozen pnpm lockfile offline. Ordinary failures restore the original manifests and lockfiles and return non-zero. Node.js, Cargo and pnpm must be available, with dependency metadata cached. Review interrupted bumps manually. No command commits, tags, pushes or publishes. See [VERSIONING.md](https://github.com/Paryx-games/roblox-manager/blob/main/VERSIONING.md) for details and limitations.

### Windows installer template

The NSIS installer uses `ram_ui/src-tauri/installer/installer.nsi`, based on Tauri Bundler 2.9.4. It adds setup and uninstall choices while retaining Tauri's install, upgrade and WebView2 handling. When upgrading the Tauri CLI, compare this template with the matching upstream version and rebuild the installer before releasing.

## Publishing a release

`.github/workflows/release.yml` runs when you push a tag matching `v*`, and can also be re-run manually against an existing tag (see [If GitHub is being stubborn](#if-github-is-being-stubborn)). Normal pushes to `main` or other branches do not publish releases.

1. Run `bump-version.bat` or `pnpm version:set X.Y.Z`. If you edited Cargo manually, run `pnpm version:sync`. Review the resulting manifest and lockfile diff.
2. Use the existing `## vX.Y.Z` section if notes were collected under the unpublished version. Otherwise rename `## Unreleased` to that heading, or add it above the previous release. Never create duplicate version headings. While v2.2.0 is pending, add current and subsequent changes directly under `## v2.2.0`; after publication, resume `## Unreleased` until the next version is chosen. The workflow requires an exact heading matching the tag.
3. If dependencies changed, regenerate and review the Cargo and pnpm lockfiles using their package managers. Do not edit lockfiles manually.
4. Run the full verification sequence in [Contributing](contributing.md), then build the release bundle:

```powershell
pnpm --dir ram_ui install --frozen-lockfile
pnpm --dir ram_ui tauri build --ci --bundles nsis -- --locked
```

5. Test the installer paths below using separate test data. Review the diff and commit the release files:

```powershell
git add Cargo.toml Cargo.lock CHANGELOG.md ram_ui/src-tauri/tauri.conf.json ram_ui/package.json ram_ui/pnpm-lock.yaml
git commit -m "chore(release): bump version to vX.Y.Z"
git tag vX.Y.Z
git push origin vX.Y.Z
```

The workflow uses Windows, stable Rust, Node 22 and the pnpm version pinned in the root `package.json` (currently 12.10.1). It installs frozen frontend dependencies, builds the embedded frontend and NSIS bundle with locked Cargo dependencies, and publishes:

- `roblox-manager-vX.Y.Z-windows-x64.exe`
- `roblox-manager-vX.Y.Z-windows-x64-setup.exe`
- `SHA256SUMS.txt`, covering both executables

Release notes combine the maintained changelog, GitHub-generated notes, download descriptions and VirusTotal results for both the direct executable and installer. Configure `VIRUSTOTAL_API_KEY` as a repository secret to enable scanning. Uploads and polling are best-effort; external-service failures do not block publication. Both executables are checksummed.

Local outputs are `target/release/rm_tauri.exe` and `target/release/bundle/nsis/Roblox Manager_<version>_x64-setup.exe`. GitHub asset naming happens later in the release job.

## Installer verification

The application and shortcuts are named **Roblox Manager**, with a per-user installation. The build wrapper selects development, alpha, beta or live icons for the app, installer and uninstaller. The sidebar source is `assets/branding/LogoThumbVertical.png`; NSIS consumes `ram_ui/src-tauri/installer/sidebar.bmp` at 164 x 314 pixels. Branding changes need a regenerated bitmap, not just a changed PNG.

Test on a separate Windows user or VM with backed-up test stores:

- Fresh install: default writable location, no app elevation, both shortcuts selected by default, and no Start menu folder selection page.
- Shortcut choices: each checkbox combination and a reinstall with existing shortcuts.
- Upgrade: newer version over an installed build, retaining accounts, aliases, groups, pins, ordering, settings and presets.
- Rename transition: an old **RM** installation is detected and setup asks for its uninstall first.
- Downgrade: older installer cannot replace a newer installed version.
- Interface reset: clears interface browser data while retaining encrypted application data.
- Uninstall: cleanup options default off; each optional browser/log cleanup does only what its wording says; open-folder works when the folder exists.
- Reinstall: retained encrypted data unlocks for the same Windows user, with custom store locations unaffected.
- Missing WebView2: bootstrapper installs the runtime with internet access; existing-runtime machines do not need a runtime replacement.
- Silent/update paths: interactive choices are skipped and optional cleanup remains off.

NSIS supports `/S` for silent setup, `/P` for passive mode, `/NS` to suppress shortcut creation, `/R` to run after silent/passive setup, and `/D=...` for the installation directory (last argument). `/UPDATE` is an updater mode, not a user data-cleanup switch. Normal silent/passive setup retains Tauri's shortcut defaults, including desktop creation unless `/NS` is supplied. Never run unattended uninstall/recovery tests against live data.

A terminal bundle build verifies compilation and packaging only. Record which interactive and runtime cases were actually tested before declaring a release verified. See [Installer and uninstall options](../getting-started/installer-options.md) for user-facing defaults and retained data.

## If GitHub is being stubborn

If the tagged commit is correct and the failure was transient, use **Actions > Release > Re-run all jobs**, or **Run workflow** and enter that existing tag.

Release jobs explicitly check out the requested tag. Fixing `main` and manually dispatching the old tag does not include the source, lockfile or changelog fix. For a source correction, publish a new verified version/tag. Move an unpublished, unused tag only after coordination; never move a tag that users or automation already rely on. The workflow refuses to replace an already published release. Retry a transient failure before publication; publish a new verified version for a source correction rather than rebuilding a distributed version with different contents.
