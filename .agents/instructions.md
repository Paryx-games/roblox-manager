# Developer & AI Agent Guide for RM (Roblox Manager)

RM is a native Windows desktop app for multi-account Roblox management. It has a Rust core (`ram_core`) and a Tauri + React/TypeScript UI (`ram_ui`). It stores live Roblox credentials, so the Security rules below are mandatory.

- Upstream: `https://github.com/Paryx-games/roblox-manager` (MIT)
- User docs: `https://roblox-manager.gitbook.io/docs` (index at `/docs/llms.txt`, every page has a `.md` version and supports `?ask=<question>`). Check there before re-explaining a documented feature.
- Target: Windows 10/11 only. Do not add cross-platform code paths unless asked.
- Stack: Rust stable (edition 2021), Tauri 2.x, React 18+ with TypeScript, Vite. Exact crate and package versions live in `Cargo.toml` and `ram_ui/package.json`.
- Package managers: always `cargo` and `pnpm`. Never hand-edit `Cargo.lock` or `pnpm-lock.yaml`.

## Before you start a task

State in one or two lines which rules apply to this task and your plan, for example: "touches `crypto.rs`, so security rules and a PR callout apply; plan: ...". Do this before editing files or running commands. Then read what the lookup table says you need.

| If the task touches...                              | Read first                                           |
| --------------------------------------------------- | ---------------------------------------------------- |
| anything in `ram_ui/frontend/`                      | `DESIGN.md`, `tokens.css`, `styles.css`              |
| opening a PR                                        | `PR_CONVENTIONS.md`                                  |
| commit messages                                     | `CONVENTIONAL_COMMITS.md`                            |
| version numbers or releases                         | `VERSIONING.md`, `docs/developers/releasing.md`      |
| cookies, auth, crypto, storage, redaction, process  | Security rules below                                 |
| a vulnerability                                     | `SECURITY.md` (never a public issue)                 |

## Current state

- The UI is Tauri + React/TypeScript. The old egui app is removed (available in Git history).
- Unfinished settings are documented in `docs/guides/settings.md`.
- Desktop browser and Windows startup helpers live under `ram_ui/src-tauri/src/`. React components and shared styles (including `tokens.css`) live under `ram_ui/frontend/`.
- The retired egui `ram_ui` Rust crate is no longer a workspace member. If you find references to it, they are stale.
- `ram_core` is open for normal development. Follow the core rules below.

## Repository layout

Top-level only; see the repo for the rest.

```text
roblox-manager/
├── Cargo.toml        # workspace manifest, source of truth for the version
├── CHANGELOG.md
├── DESIGN.md         # UI design system
├── PR_CONVENTIONS.md
├── assets/           # static assets, assets/icons for all icons
├── docs/             # guides and developer docs
├── ram_core/         # headless core library, no UI dependencies
│   ├── src/          # models, crypto, storage, auth, api, process, redact, error, ...
│   └── tests/        # integration tests (e.g. device_store.rs)
└── ram_ui/
    ├── src-tauri/    # tauri commands, window/tray setup, calls into ram_core
    └── frontend/     # React/TypeScript UI, tokens.css, styles.css, components/, lib/ipc.ts
```

### Module map

`ram_core/src/`:

- `lib.rs` crate root and exports, `error.rs` `CoreError`
- `models.rs` Account, AccountStore, AppConfig, LaunchPreset, etc.
- `crypto.rs` envelope encryption (Device and Password modes), Argon2id, AES-256-GCM
- `storage.rs` crash-safe persistence (`atomic_write`, `atomic_swap`, `.bak`)
- `auth.rs` `RobloxClient` with per-cookie CSRF token caching and exponential backoff
- `api.rs` Roblox REST APIs (avatars, presence, place info, user profiles)
- `group_api.rs` group announcements and forums
- `accounts.rs` reusable account validation and merge rules
- `assets.rs` Asset Manager data structures, state machines, validation
- `assets_api.rs` Open Cloud and asset upload APIs, `multipart.rs` custom multipart encoder
- `instances.rs` `InstanceRegistry` and exact launchtime token attribution
- `presets.rs` per-file preset persistence (`presets/<slug>.json`)
- `process.rs` Win32 process discovery, singleton mutex holding, launching, window tiling
- `redact.rs` log redaction rules (cookies, auth tickets, CSRF tokens, user paths)

`ram_ui/src-tauri/src/`:

- `main.rs` entry point, CLI dispatcher, logger setup
- `accounts.rs` account commands and credential validation
- `instances.rs` running client attribution and process actions
- `launcher.rs` shared launch coordination and pacing
- `asset_manager.rs` developer creation and upload commands
- `lifecycle.rs` startup, migrations, recovery
- `login.rs` isolated Tauri login window
- `browser_login.rs` isolated browse-as subprocess and compatibility login mode
- `startup.rs` Windows startup registration
- `state.rs` managed app state (account store handle, instance registry, etc.)

`ram_ui/frontend/`:

- `main.tsx` entry, `App.tsx` root shell (currently 36px rendered title bar, 56px navigation rail, routed pages)
- `tokens.css` shared color, typography, spacing, motion tokens
- `styles.css` Tailwind import, theme mapping, page and component styling
- `components/` account picker, custom select, popups, tooltips, icons, loading skeletons
- `*Page.tsx` Accounts, Instances, Groups, Private Servers, Presets, Inventories, Assets, Settings
- `lib/ipc.ts` typed wrappers around Tauri commands, `hooks/` shared hooks (e.g. live presence subscription)

This list can drift. If it disagrees with the repo, trust the repo and fix this file.

## Architecture

### Where code goes

- `ram_core`: pure logic, cryptography, Roblox HTTP, Win32 process work, persistence. No dependency on Tauri, React, WebView2, frontend DTOs, or `AppState`.
- `ram_ui/src-tauri`: thin `#[tauri::command]` handlers, event emission, IPC validation, window lifecycle, browser cookie capture, and safe error shaping for the frontend. Business logic belongs in `ram_core`, not in a handler.
- `ram_ui/frontend`: presentation only. It talks to Rust through the typed wrappers in `lib/ipc.ts`, never ad hoc `invoke()` calls in components.
- Long-running or streaming work (presence polling, launch progress, upload progress) uses Tauri events (`emit`/`listen`), not frontend polling.
- Never block the command thread on network, heavy disk I/O, Win32 work, or credential-store work. Use async handlers for async I/O and `tauri::async_runtime::spawn_blocking` for blocking work. Use spawned background tasks only when the work genuinely outlives one command. On the frontend every IPC call is async and components handle pending and error states explicitly.
- Surface frontend errors through the page's visible error, retry, or toast behavior, never a console-only failure.

### Concurrency and task ownership

- `AppState.runtime`, `pending_additions`, and `instances` use `std::sync::Mutex`. Keep those guards short and **never carry a synchronous mutex guard across an `.await`**. Copy or clone the data needed for async work, release the guard, then await.
- Blocking filesystem, Win32, dialog, and credential-store work belongs in `tauri::async_runtime::spawn_blocking`; network work stays async.
- `account_refresh` and `launch_queue` use `tokio::sync::Mutex` for async coordination. Holding an async mutex across an `.await` is allowed only when serialising the whole operation is the point. `launch_queue` intentionally does this for launch pacing. Do not mechanically replace synchronous state locks with async locks.
- Every spawned long-lived task needs an owner and a shutdown story. App-wide loops are owned by `BackgroundTasks` and are aborted on `stop`/`Drop`. Feature tasks must be cancellable or cleaned up when their owning window/state goes away.
- After an `.await`, assume state may have changed. Before committing results, re-check relevant invariants such as the account still existing, the credential revision still matching, the selected asset/account still owning the operation, and shutdown not having started.
- For operations with multiple side effects, decide the partial-failure and rollback behaviour before coding. Prefer candidate-state/commit patterns so a failed save does not leave memory and disk disagreeing.

### Core rules

- Keep existing public core APIs compatible unless the task calls for a change. If you break one, update every caller in the same change.
- Do not change on-disk formats, paths, schema versions, or defaults without a migration and an explicit mention in the PR.
- Prefer small, targeted changes. No drive-by refactors.
- Changes to `crypto.rs`, `storage.rs`, `redact.rs`, `auth.rs`, or `process.rs` need focused tests with synthetic data and a callout in the PR description. Never weaken the protections they provide.
- Core errors use `ram_core::error::CoreError` (`thiserror`). `src-tauri` maps them to safe serializable errors for the frontend (currently usually `Result<_, String>`), without leaking secrets or unnecessary internal detail.

### Encryption and storage

- Accounts and cookies are never stored in plaintext. `accounts.dat` is encrypted with a random AES-256-GCM data key wrapped in the store header.
- `StoreMode::Device` (default): wrapping key lives in Windows Credential Manager via `keyring`. `StoreMode::Password`: wrapping key is derived with Argon2id from a master password.
- All writes of persisted state (`accounts.dat`, `config.json`, presets) go through `ram_core::storage::atomic_write` or `atomic_swap`, including from `src-tauri`. No bare `std::fs::write` on state files.

### Persistence and migration rules

There is **no single repository-wide schema version**. Use the compatibility mechanism of the format you are changing.

- `config.json` is `AppConfig` JSON. New compatible fields normally use `#[serde(default)]` or a default function and are added to `Default`. Add a test that loads an older/minimal config. Renames, removals, enum-shape changes, path changes, or default changes that alter existing installs need an explicit migration before the new state is saved.
- `accounts.dat` has its own envelope format in `crypto.rs` (`RAMSTORE`, currently format v2). Legacy v1 is upgraded only after a successful unlock, and the upgrade is all-or-nothing. Any incompatible account-store format change needs a format-version decision, loader/migration logic, old-format tests, and explicit downgrade behaviour.
- `AssetIndex` has its own `CURRENT_SCHEMA`; presets and `AppConfig` do not inherit it. Bump a schema only when that format's compatibility semantics require it, not just because a Rust struct gained a field.
- `atomic_write` preserves the outgoing primary as `<path>.bak`. A migration must define what happens to that backup. When key material changes, use the existing rekey path (`save_rekeyed`) so the backup is also rewritten under the new key instead of remaining readable with retired credentials.
- Downgrade readability is format-specific and is **not guaranteed**. If an older RM version will be unable to read state written by the new version, make that deliberate, test it where practical, and call it out in the PR/release notes.
- Migrations must be crash-safe and retry-safe: never destroy the only good copy before the replacement is durable, and avoid a second run duplicating or corrupting migrated data.
- Migration tests use synthetic old-format data and cover the failure path as well as the happy path. Where backup recovery matters, test the primary and `.bak` behaviour together.

### Instance tracking and launching

- Roblox starts through the `roblox-player:` protocol. RM stamps a `+launchtime:<millis>+` token into the launch URI, and the spawned `RobloxPlayerBeta.exe` keeps it in its command line.
- RM reads the command line via `OpenProcess` + `ReadProcessMemory` for exact attribution (`Attribution::Exact`), falling back to FIFO appearance order (`Attribution::Inferred`).
- Multi-instance: RM holds `ROBLOX_singletonMutex` and the legacy `ROBLOX_singletonEvent` in its own process. It never closes mutex handles inside Roblox processes.
- Window tiling places running Roblox windows into a grid using monitor geometry.
- Privacy mode clears `%LOCALAPPDATA%\Roblox\LocalStorage\RobloxCookies.dat` before launch.

### Webview architecture

- Tauri hosts the React UI in one managed WebView2 per window; there is no re-exec subprocess for the main UI.
- For flows that need isolation (embedded Roblox login, browse-as), try a secondary `WebviewWindow` first. Only use a re-exec'd child process if there is a concrete isolation need a secondary window cannot meet, and document why.
- Either way this is a security boundary: anything coming out of a login or browse-as window goes through the same redaction and storage rules as everything else.

### Logging

- Daily rotating logs at `%APPDATA%\RM\rm.<YYYY-MM-DD>.log`.
- `ram_core::redact::scrub` and `ScrubbingWriter` redact `.ROBLOSECURITY` cookies, `gameinfo:` tickets, CSRF tokens, and Windows username paths at write time. Command handlers in `src-tauri` are bound by the same rules.

## Security rules

**RM's value is not leaking credentials. Treat this section as mandatory.**

- Never log `.ROBLOSECURITY` cookies, session tickets, master passwords, CSRF tokens, or other credentials.
- Never put secrets in user-facing errors, debug output, panic messages, `tracing` fields, URLs, query parameters, telemetry, or IPC error payloads. Error variants carry an identifier (account alias, request ID), never the secret. An `Err(CoreError::Auth(format!("request failed: {cookie}")))` is a credential leak, not a convenience. A secret serialized into an IPC error is a leak even if it never hit a log.
- Keep secrets' lifetimes small. Pass references, scope them to the function that needs them, and do not clone or thread them through unrelated layers. Do not send raw secrets across the IPC boundary when the frontend only needs to know an account is authenticated.
- Do not weaken, bypass, or remove encryption, credential-store, redaction, or privacy protections unless the task explicitly requires it, and call it out clearly if it does.
- Treat all of these as untrusted and validate them before use in filesystem paths, process arguments, network requests, or security decisions: Roblox REST/Open Cloud responses, user input, imported presets and config, anything the frontend sends to a command, and anything read via `ReadProcessMemory` (including launch tokens).
- Launch-attribution tokens (`+launchtime:<millis>+`) and other process data are untrusted: validate their shape before use and never let them reach logs unredacted.
- Do not replace `aes-gcm`, `argon2`, `sha2`, `rand`, or `keyring`, and do not hand-roll crypto, without explicit approval.
- When adding a new secret pattern or URI parameter, update `ram_core::redact` in the same change.
- Never commit credentials, cookies, tokens, account data, logs, or memory dumps. Test fixtures use synthetic values, never real captured ones, even redacted.
- Secrets reach the clipboard only through an explicit user action, never as a side effect. Prefer copying the least sensitive identifier that does the job (an alias over a cookie).

## Roblox API reliability

Roblox HTTP is a core dependency of RM, but it is still an external contract. Some endpoints are undocumented or change independently of this app. Code as if fields, status bodies, enum values, and rate limits can surprise you.

- Reuse `ram_core::auth::RobloxClient` for cookie-authenticated requests. Do not add a second CSRF cache, retry loop, or ad hoc rate-limit sleep around it.
- Preserve the existing status distinctions: a `403` with `x-csrf-token` is a CSRF rotation and is retried; a `403` without that challenge becomes `CookieRejected`/`CookieRejectedWithReason`; `429` uses `Retry-After` when present or the existing jittered exponential backoff and eventually becomes `RateLimited`. Do not collapse these into one generic "auth failed" path.
- Treat `401` and other authentication failures separately from CSRF and rate limiting. Only mark a credential expired when the endpoint/result actually supports that conclusion; do not turn every non-2xx into "cookie expired".
- Check status before parsing a success payload. Empty bodies, HTML/proxy error pages, malformed JSON, missing optional fields, and newly-added enum values must return a safe error or degrade to an `Unknown`/`Other`-style value where that makes sense; they must never panic.
- Prefer optional/defaulted fields for data Roblox may omit. For externally controlled enums that can grow, use a catch-all instead of making one unknown value fail the entire response when the feature can continue safely.
- Do not implement an endpoint from memory. Check the current official documentation where it exists, or a current real response with all secrets and personal data removed before using it as a shape reference.
- API tests use synthetic responses shaped like the real contract. Include the edge case the code claims to tolerate: missing optional fields, unknown values, empty/non-JSON error bodies, pagination cursors, or partial results as applicable. Never commit captured cookies, auth tickets, CSRF tokens, webhook URLs, or account data.
- Respect endpoint batch/page limits and existing batching patterns. Do not "fix" rate limits by firing more concurrent requests.
- A retry inside `RobloxClient` already consumes time. Do not wrap it in another blind retry loop unless the feature has a separate, bounded reason to retry and the combined worst-case delay is understood.

## UI work

**Read `DESIGN.md`, `tokens.css`, and `styles.css` before touching `ram_ui/frontend/`.** Summary:

- Main window is 1200 x 760 (also the minimum), with the current 36px rendered title bar and a 56px navigation rail. `DESIGN.md` records that `--titlebar-height` still says 40px; treat the rendered stylesheet and `DESIGN.md` as authoritative until that stale token is reconciled.
- Status color is reserved for live account, instance, and process state. Never decoration, never a button.
- No shadows, one radius, one border weight, Roboto for interface text.
- Use shared tokens and existing components from `components/`. Do not create a component for a single-use wrapper. `styles.css` maps a small set of tokens into Tailwind utilities, but most page styling uses named CSS classes.
- Every interactive component needs its full state set (hover, focus, active, disabled, loading as applicable). A button with no focus ring is incomplete.
- Icons come from `assets/icons` through the local icon component. No emoji as icons.
- Stylelint blocks raw hex colors and `!important`. The rest is checked in review, so check it yourself.
- If you need a pattern DESIGN.md does not cover (new card style, new table variant), stop and ask before inventing it.
- Accessibility is part of UI correctness: follow `DESIGN.md` for keyboard navigation, focus order/visibility, accessible names, reduced motion, contrast/readability, and loading/error semantics. Reuse the behavior already built into shared components instead of reimplementing it per page.

## Commands

Run from the workspace root on Windows (PowerShell).

```powershell
pnpm --dir ram_ui install          # install frontend deps
pnpm --dir ram_ui tauri dev        # run the app in dev mode
pnpm --dir ram_ui tauri build --ci --bundles nsis -- --locked   # release bundle
# single tests
cargo test -p ram_core <test_name>
cargo test --manifest-path ram_ui/src-tauri/Cargo.toml <test_name>
```

### Tests

- Unit tests live in a `#[cfg(test)]` module next to the code, in both `ram_core` and `src-tauri`.
- Integration tests go in `ram_core/tests/`.
- New behavior gets a test. Use synthetic data only.

### Verification

Per commit (fast):

```powershell
cargo fmt --all -- --check
cargo check
pnpm --dir ram_ui typecheck
```

Before pushing or opening a PR (full local verification; covers CI's source checks, packaged Tauri build excluded):

```powershell
pnpm --dir ram_ui version:check
pnpm --dir ram_ui lint
pnpm --dir ram_ui typecheck
pnpm --dir ram_ui test
pnpm --dir ram_ui build
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

UI, launch, account, credential, storage, and process changes also need manual testing on Windows with Roblox installed. If you cannot do that (cloud agent, no Windows, no Roblox), say so, leave the manual item unchecked in the PR Testing section with an explanation, and do not claim it was verified. That is a reportable limitation, not a blocker for opening the PR.

## Common change recipes

### Add a Tauri command

1. Put the command in the existing feature module when one owns the behaviour; keep business logic in `ram_core`.
2. Validate every IPC argument before using it in paths, process arguments, URLs, or security decisions.
3. Keep the handler thin. Await network work normally; move blocking filesystem/Win32/keyring/dialog work into `tauri::async_runtime::spawn_blocking`.
4. Register the command in `tauri::generate_handler![...]` in `ram_ui/src-tauri/src/main.rs`.
5. Add or update the typed wrapper in `ram_ui/frontend/lib/ipc.ts`; components do not call `invoke()` directly.
6. Give the UI explicit pending/success/error behaviour. Use events for long-running progress rather than polling a command.
7. Add focused Rust tests for validation/business logic and frontend tests where wrapper or UI behaviour changed. Re-check redaction if the command touches credentials, URLs, or external error text.

### Add a persisted field

1. Identify the actual format first: `AppConfig`, encrypted `AccountStore`, `AssetIndex`, or a standalone preset. Do not invent a global schema bump.
2. Make old data load safely. For compatible JSON additions, add the appropriate serde default and update `Default`; for incompatible changes, write a migration.
3. Route writes through the format's existing save path (`AppConfig::save`, `crypto::save_store`/`save_rekeyed`, `AssetIndex::save`, preset helpers), which ultimately uses atomic storage.
4. Decide downgrade behaviour and `.bak` behaviour explicitly if the representation or key material changes.
5. Add a test that loads synthetic data in the previous shape and proves the new code preserves existing values/defaults correctly.

### Add a frontend page/workspace

1. Read `DESIGN.md`, `tokens.css`, and `styles.css`.
2. Add the page component and wire it into `App.tsx` using the existing routed-workspace pattern.
3. Add the navigation-rail entry. If visibility is configurable, update the existing page-visibility model/defaults and the Settings control rather than inventing a second toggle path.
4. Reuse existing components/tokens and implement loading, empty, error, retry, disabled, and keyboard/focus states that apply.
5. Add/update IPC wrappers and backend commands only when the page needs new data or actions.
6. Update the relevant user guide and `CHANGELOG.md` if the workspace or behaviour is user-visible.
7. Test at the supported minimum window size and record the manual Windows check.

### Add a Roblox endpoint

1. Put general REST work in `api.rs`, group-specific work in `group_api.rs`, and asset/Open Cloud work in `assets_api.rs`.
2. Verify the current endpoint, method, authentication, request fields, response shape, pagination/batch limits, and documented rate-limit behaviour.
3. Reuse `RobloxClient` when cookie auth/CSRF applies; do not duplicate its retry logic.
4. Model the response defensively with optional/defaulted fields and unknown-value handling where safe.
5. Convert non-success responses into existing `CoreError` categories without leaking response secrets.
6. Add synthetic parser/behaviour tests, including at least one malformed/partial/unexpected-shape case relevant to the endpoint.
7. Check redaction before logging any new URL parameter, header, identifier, or error text.

### Add a setting

1. Add the field to `AppConfig` with the correct serde/default behaviour.
2. Add it to the Tauri settings DTO/update path (`SettingsConfig`/`SettingsUpdate` and conversion/apply code) instead of reading config directly from React.
3. Add the control to `SettingsPage.tsx` using the existing save/discard/navigation-guard behaviour and design components.
4. Add/update an info card in `infocards.json` when the setting needs explanation or warning text.
5. Update `docs/guides/settings.md`.
6. Add tests for validation/default/back-compat behaviour and update `CHANGELOG.md` when users will notice the capability or behaviour.

## Git and PR workflow

RM uses a fork-based flow. Never push to upstream `main` and never work directly on `main` for a change you plan to submit.

```powershell
git clone https://github.com/GITHUB_USERNAME/roblox-manager.git
cd roblox-manager
git config core.hooksPath .githooks
git remote add upstream https://github.com/Paryx-games/roblox-manager.git
git switch -c your-feature-name
```

- Setting `core.hooksPath` and adding the `upstream` remote right after cloning is the one allowed git config change. Beyond that, never modify git config or remotes, change origin, force-push, reset, or discard unrelated work.
- To sync: `git fetch upstream`, `git switch main`, `git pull --ff-only upstream main`, `git push origin main`. Rebase existing feature branches onto the synced `main` instead of merging.
- Commits follow `CONVENTIONAL_COMMITS.md`, for example `fix(auth): preserve csrf token per cookie` or `feat(groups): show membership status`. Commit each logical change as soon as it is done, not one giant commit at the end. Do not batch unrelated changes. Run the fast verification before each commit.
- Review the diff before committing: only task-relevant changes, no cookies, tokens, logs, build output, `node_modules/`, local account data, or stray files.
- One focused change per PR. Follow `PR_CONVENTIONS.md` for title, Changes, Testing, and the Checklist. Include design-system confirmation for UI changes, and call out security-sensitive changes and Roblox-version assumptions.

### Changelog

If a commit changes something a user would notice (feature, fix, behavior, UI), add an entry under `## Unreleased` in `CHANGELOG.md` in the same commit. Create the heading above the latest `## vX.Y.Z` if it does not exist. Skip internal refactors, tests, comments, docs, CI, and dependency bumps with no behavior change.

Exception: while the v2 Tauri/React page rewrite is in progress, add one consolidated entry when a page is complete instead of one per incremental fix.

### Docs sync

- A user-visible workflow or behaviour change must update the matching page under `docs/` in the same change when documentation for that area exists. `CHANGELOG.md` records that something changed; it does not replace the guide that explains how the feature works.
- Settings changes update `docs/guides/settings.md`. Installer/runtime changes update the getting-started/installer docs. Security, storage, login, or privacy behaviour updates the corresponding security/guide page.
- Pure refactors, tests, CI changes, dependency bumps with no behaviour change, and trivial visual polish do not need user-doc churn.

### Versioning

SemVer `MAJOR.MINOR.PATCH`, with `VERSIONING.md` as the full policy.

- **MAJOR**: breaking changes requiring migration, or a substantial new generation of RM through significant architectural work or material scope changes. Effort alone is not enough; follow the milestone criteria in `VERSIONING.md`.
- **MINOR**: backward-compatible features, settings, tabs, capabilities, or feature removals.
- **PATCH**: bug fixes, wording changes, UI polish. Never adds capability.

There is no project-wide `0.x` or beta phase; every line ships stable. Pre-release identifiers stage a version before its plain tag:

- `-alpha.N`: early, unfinished build; expect breakage.
- `-beta.N`: early test build; may lack features or contain bugs.
- `-rc.N`: release candidate; if clean, publish the plain version next. An `-rc.N` requires an earlier `-alpha.N` or `-beta.N` for that version, otherwise publish the plain version directly.

Pre-releases sort before their plain release (`v1.2.3-rc.1` < `v1.2.3`), so tooling must never treat a pre-release as latest stable. Group related changes into appropriate releases instead of cramming unrelated changes into one version.

The root `Cargo.toml` is the version source of truth. Both Rust crates inherit it, Tauri derives it (no `version` override in `tauri.conf.json`), and `ram_ui/package.json` is the only mirror. Use `pnpm version:set <version>` or `bump-version.bat`, `pnpm version:sync` after manual Cargo edits, and `pnpm version:check` to validate. Do not add other version constants.

### Publishing a release

`.github/workflows/release.yml` runs on pushed `v*` tags and on manual `workflow_dispatch` for an existing tag. Normal pushes to `main` never publish. Both release jobs check out the selected tag. See also `docs/developers/releasing.md`.

1. Run `bump-version.bat` or `pnpm version:set <version>` from the repo root to bump Cargo and sync the frontend mirror and lockfiles. If Cargo was edited manually, run `pnpm version:sync`.
2. Rename `## Unreleased` in `CHANGELOG.md` to `## vX.Y.Z`. If there is no Unreleased section, add `## vX.Y.Z` above the previous release. The workflow fails if no heading matches the tag exactly.
3. If dependencies changed, sync the lockfiles and check the diffs are expected:

   ```powershell
   cargo build
   git diff Cargo.lock
   pnpm --dir ram_ui install
   git diff ram_ui/pnpm-lock.yaml
   ```

   Both lockfiles must be in the release commit. The workflow uses `--locked` and `--frozen-lockfile` and fails on a stale one.

4. Commit the release files:

   ```powershell
   git add Cargo.toml Cargo.lock CHANGELOG.md ram_ui/src-tauri/tauri.conf.json ram_ui/package.json ram_ui/pnpm-lock.yaml
   git commit -m "chore(release): bump version to vX.Y.Z"
   ```

5. Tag and push:

   ```powershell
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

6. The workflow builds and publishes automatically. It renames the installer and exe to `roblox-manager-vX.Y.Z-windows-x64` and adds changelog content, GitHub-generated notes, a downloads table, and a SHA256 checksum.

Retrying a release: if the tagged commit is correct and the failure is transient, use Actions > Release > Re-run all jobs, or manually dispatch the existing tag. A fix pushed only to `main` is not included, because the jobs check out the tag. For source fixes, publish a new verified version and tag. Move an unpublished, unused tag only after coordination, and never move one that users or automation already rely on.

### Installer maintenance

The NSIS template under `ram_ui/src-tauri/installer/` is based on Tauri Bundler 2.9.4. Compare it with upstream when upgrading Tauri. Build with `pnpm --dir ram_ui tauri build --ci --bundles nsis -- --locked`, then verify on a separate Windows test setup: fresh install, upgrade, shortcut choices, optional cleanup, and missing-WebView2 cases.

## Coding standards

- Preserve existing behavior unless the task explicitly requires changing it. Prefer small, targeted changes over refactors.
- Comments explain intent, not syntax. Lowercase, concise, only where the why is not obvious.
- Use `PathBuf` and clean path handling. No hardcoded Unix assumptions.
- Do not create unrequested files, especially summary or report `.md` files. Summaries go in the chat, PR description, and changelog. Remove any scratch files you created before finishing.

### Dependency policy

- Prefer the standard library and dependencies already in the workspace. Do not add a crate or npm package for a tiny helper that is clearer to implement locally.
- A new dependency needs a concrete reason in the PR: what existing code cannot reasonably provide, and any meaningful binary-size, native-runtime, network, security, or maintenance impact.
- Follow the existing workspace/package layout. Let `cargo`/`pnpm` update lockfiles; never hand-edit `Cargo.lock`, root `pnpm-lock.yaml`, or `ram_ui/pnpm-lock.yaml`.
- Keep dependency additions targeted. Do not combine a feature with broad unrelated upgrades, and do not accept a transitive dependency change blindly without reviewing the lockfile diff.
- Removing a dependency is also a behaviour/build change: verify every platform/build path that used it before declaring the cleanup done.

## Editing safely

The principle: verify the diff, not the exit code. A clean `cargo check` or `pnpm typecheck` means the code is valid, not that your intended change is present. Missing UI, a dropped section, or an orphaned block will not fail a type check.

- Prefer targeted edits anchored on unique surrounding text over line-number surgery.
- After any large deletion, insertion, or rewrite, re-read the affected region.
- If a full-file replacement seems warranted, say so and confirm before doing it, instead of announcing it and then continuing with risky line edits.

Known pitfalls:

- Do not re-run a command that already gave a clean result on a hunch (for example `cargo fmt --all` right after a passing `--check`). Investigate the discrepancy instead.
- You are on PowerShell. `head`, `tail`, and other Unix tools may not exist. Fix a wrong-shell command once and adapt for the rest of the session.

### Failure-path pass before "done"

Before marking a change complete, trace the failure paths that are realistic for that feature, not just the happy path. At minimum consider:

- stale state after an `.await` (account removed/re-added, credential revision changed, selected row/page changed)
- duplicate invocation or double-clicks, plus concurrent background refreshes
- cancellation, window closure, or app shutdown while work is in flight
- partial success where an external side effect succeeds but persistence/UI update fails, or vice versa
- save failure and rollback: whether memory, primary state, and `.bak` still agree
- retries and terminal failures from Roblox, including auth rejection, CSRF rotation, rate limiting, timeout, and malformed/non-JSON responses
- cleanup after spawned tasks, event listeners, timers, temporary files, process handles, and privacy backups

State in the final response which relevant failure paths you checked. If one could not be exercised, say so instead of implying it was covered.

## When to stop and ask

Explain the impact and get confirmation before:

- changing an on-disk format, schema, or doing a destructive migration
- a significant architectural change or removing existing functionality
- a new design-system pattern not covered by `DESIGN.md`
- swapping a crypto or credential-storage crate
- any change that would weaken a security protection
- needing a secret to cross the IPC boundary to make something work

## Troubleshooting known environment failures

- **WebView2 missing/broken:** the Tauri UI and login windows require Microsoft Edge WebView2 Runtime. The installer can bootstrap it when missing if internet access is available; the portable executable expects the runtime to already exist. A WebView2 failure is not evidence that account data was reset.
- **Device-store tests:** `ram_core/tests/device_store.rs` intentionally talks to the real OS credential store. It may create the normal device key, never deletes that machine-wide key, and skips when the credential store is unavailable. Do not rewrite those tests to destructively "clean up" the real key.
- **Windows executable locked:** if a build/link step cannot replace `target\debug\rm_tauri.exe` or `target\release\rm_tauri.exe`, first check whether the dev/release app is still running. Close the process instead of deleting build directories or resetting unrelated state.
- **Wrong shell:** repository commands assume PowerShell on Windows. If a Unix-only command fails, translate it once and continue with the PowerShell equivalent rather than repeatedly retrying it.

## Glossary

- **attribution:** mapping a running `RobloxPlayerBeta.exe` to the account/launch that created it. `Exact` comes from the `+launchtime:<millis>+` token; `Inferred` is the fallback ordering.
- **store mode:** how the encrypted account store's data key is wrapped: `Device` via Windows Credential Manager or `Password` via Argon2id.
- **privacy mode:** the configured pre-launch/exit cleanup of Roblox local state. It is not anonymisation and does not change the Roblox account's public identity.
- **preset:** a saved launch definition stored as its own JSON file under the presets directory.
- **singleton mutex:** the Roblox singleton objects RM holds (`ROBLOX_singletonMutex` plus the legacy `ROBLOX_singletonEvent`) so multiple Roblox clients can coexist. RM does not close mutex handles inside Roblox processes.

## Definition of done

- The change does what was asked, and the actual diff matches your intent (not just a green build).
- Relevant failure paths were traced before completion, and the final response says which ones were checked.
- Tests added or updated for new behavior, using synthetic data.
- Full verification passes, or any gap is stated plainly.
- Manual Windows checks done where required, or reported as not done.
- `CHANGELOG.md` updated if user-facing.
- Matching user documentation updated when the workflow/behaviour changed and a guide exists.
- Security rules re-checked for anything touching secrets, storage, or process data.
- UI changes checked against `DESIGN.md`.
- No stray files, no unrelated changes, commits are small and conventional.
- PR follows `PR_CONVENTIONS.md`.

## Final response format for agents

End the task with a compact report containing:

- **Changed:** what actually changed, grouped by behaviour rather than file list.
- **Verified:** exact automated checks and manual checks that passed.
- **Not verified:** anything required/relevant that could not be run, with the reason.
- **Noticed but not fixed:** relevant follow-up problems discovered outside the requested scope. Do not silently fix them and do not omit them if they materially affect the result.

Do not claim a path was tested because the code looks correct, because another check passed, or because CI is expected to cover it later.
