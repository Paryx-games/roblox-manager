# AGENTS.md - Developer & AI Agent Guide for RM (Roblox Manager)

## Pre-task acknowledgement (required)

Before beginning repository work, briefly acknowledge that this file has been read and that its instructions will be followed.

Use:

> Understood pre-task rules

or a close equivalent. This acknowledgement is required before editing files, running commands, or making other repository changes. **The security rules under Agent Guidelines § 5 are non-negotiable - RM stores live Roblox credentials, so treat that section as load-bearing, not advisory.** **If your task touches any UI code (`ram_ui/src/`), you must also read `DESIGN.md` and `ram_ui/src/tokens.css` before writing a single component - see Agent Guidelines § 10.**

## Project Overview

**RM (Roblox Manager)** is a fast, lightweight, native Windows desktop application built with a **Rust core** and a **Tauri + React/TypeScript** UI. It provides comprehensive Roblox multi-account management, secure credential storage, multi-instance game launching, automated window tiling, live presence tracking, group management, asset uploading, and anti-association privacy features.

- **Upstream Repository**: `https://github.com/Paryx-games/roblox-manager`
- **Docs Site**: `https://roblox-manager.gitbook.io/docs` (llms.txt index at `/docs/llms.txt` - every page has a `.md` version and supports `?ask=<question>` for live querying)
- **License**: MIT
- **Primary Target OS**: Windows 10 / 11 (uses Win32 APIs, Windows Credential Manager, and Tauri's WebView2-based webview)
- **Rust Edition**: 2021 (Rust stable 1.75+)
- **Frontend**: React 18+ with TypeScript, built with Vite

> **Migration note**: this branch replaces the legacy `egui`/`eframe` UI (`ram_ui` as a native immediate-mode GUI) with a Tauri shell hosting a React/TypeScript frontend. `ram_core` (the headless Rust logic/crypto/API layer) is unchanged by this migration - see Architecture below.

---

## Tech Stack & Key Crates

| Category                       | Technology / Crates                                                                                     |
| ------------------------------ | ------------------------------------------------------------------------------------------------------- |
| **Desktop Shell**              | `tauri` (2.x), `tauri-plugin-*` as needed (dialog, fs, shell)                                           |
| **Frontend**                   | React 18+, TypeScript, Vite, `stylelint` (design-token enforcement - see Design System below)           |
| **Async Runtime & Networking** | `tokio` (1.x, full), `reqwest` (0.12 with json & cookies)                                               |
| **Serialization & Errors**     | `serde` (1.x derive), `serde_json` (1.x), `thiserror` (2.x)                                             |
| **Cryptography & Security**    | `aes-gcm` (0.10, AES-256-GCM), `argon2` (0.5, Argon2id), `sha2`, `rand`, `keyring` (3.x windows-native) |
| **Windows System & APIs**      | `windows-sys` (0.59: Process, Threading, Diagnostics, Security, UI, DWM), `sysinfo` (0.32)              |
| **Logging & Diagnostics**      | `tracing` (0.1), `tracing-subscriber` (0.3), `tracing-appender` (0.2 daily rolling)                     |
| **Native Dialogs**             | Tauri's `dialog` plugin (replaces `rfd`)                                                                |

Always use `cargo` for anything crate/build-related - never hand-edit `Cargo.lock`. Always use `pnpm` for anything frontend-package-related - never hand-edit `pnpm-lock.yaml`.

---

## Workspace & Project Architecture

The project is structured as a Cargo workspace with two main pieces: a headless core crate, and a Tauri app containing both the Rust command layer and the React frontend.

```
robloxmanager/
├── Cargo.toml                 # Workspace manifest & dependencies - SOURCE OF TRUTH FOR VERSION
├── CHANGELOG.md                # User-facing changes, grouped by ## vX.Y.Z release
├── SECURITY.md                 # Vulnerability reporting - use this, not public issues
├── CONVENTIONAL_COMMITS.md     # Commit message format reference
├── VERSIONING.md               # SemVer policy reference
├── DESIGN.md                  # UI design system spec - REQUIRED READ before any UI change
├── .github/workflows/release.yml # Tag-triggered release pipeline (v* tags only)
├── assets/                    # Static assets (e.g. Logo.png, assets/icons for all icon use - see DESIGN.md § Icons)
├── ram_core/                  # Headless core library (logic, APIs, crypto, Win32) - UNCHANGED by the Tauri migration
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs             # Core crate root & exports
│   │   ├── models.rs          # Data models: Account, AccountStore, AppConfig, LaunchPreset, etc.
│   │   ├── crypto.rs          # Envelope encryption (Device & Password modes), Argon2id, AES-256-GCM
│   │   ├── storage.rs         # Crash-safe atomic persistence (atomic_write, atomic_swap, .bak)
│   │   ├── auth.rs            # RobloxClient with per-cookie CSRF token caching & exponential backoff
│   │   ├── api.rs             # Roblox REST APIs (avatars, presence, place info, user profiles)
│   │   ├── group_api.rs       # Roblox Group API endpoints
│   │   ├── assets.rs          # Asset Manager data structures, state machines, validation
│   │   ├── assets_api.rs      # Roblox Open Cloud & Asset upload APIs
│   │   ├── multipart.rs       # Custom multipart body encoder for uploads
│   │   ├── instances.rs       # InstanceRegistry & exact launchtime token attribution
│   │   ├── presets.rs         # Per-file preset persistence (presets/<slug>.json)
│   │   ├── process.rs         # Win32 process discovery, mutex patching, game launching, window tiling
│   │   ├── redact.rs          # Log redaction rules (cookies, auth tickets, CSRF tokens, user paths)
│   │   └── error.rs           # CoreError definitions
│   └── tests/
│       └── device_store.rs    # Integration tests for Windows Credential Manager / Device store
└── ram_ui/                    # Tauri application
    ├── src-tauri/              # Rust side: Tauri commands, window/tray setup, calls into ram_core
    │   ├── Cargo.toml
    │   ├── tauri.conf.json
    │   └── src/
    │       ├── main.rs         # Entry point, CLI dispatcher, logger setup
    │       ├── commands/       # #[tauri::command] handlers, one module per domain (accounts, instances, groups, assets, presence)
    │       └── state.rs        # Managed app state (account store handle, instance registry, etc.)
    └── src/                    # React/TypeScript frontend
        ├── main.tsx            # Entry point
        ├── App.tsx             # Root layout: activity bar + sidebar + routed content (see DESIGN.md § Layout shell)
        ├── tokens.css           # Design tokens - SINGLE SOURCE OF TRUTH for color/type/space/motion, see DESIGN.md
        ├── components/          # Shared, reusable UI components (PageHeader, DataTable, Card, Button, StatusDot, Badge, etc.)
        ├── pages/                # One module per sidebar section: dashboard, accounts, instances, groups, assets, activity, settings
        ├── lib/
        │   └── ipc.ts            # Typed wrappers around Tauri `invoke()` calls into src-tauri/commands
        └── hooks/                # Shared React hooks (e.g. live presence subscription)
```

---

## Core Concepts & Architectural Patterns

### 1. Separation of Core, Command Layer, and UI

- `ram_core` contains zero UI dependencies. All pure logic, cryptographic operations, Roblox HTTP communication, and Win32 process manipulation reside in `ram_core`. **This crate and its module boundaries are unchanged by the Tauri migration** - the rewrite replaces the presentation layer, not the domain logic.
- `ram_ui/src-tauri` hosts thin `#[tauri::command]` handlers that call into `ram_core` and return serializable results (or emit events) to the frontend. Commands should stay thin - business logic belongs in `ram_core`, not in a command handler.
- `ram_ui/src` (React/TypeScript) is the presentation layer. It calls into the Rust side exclusively through typed wrappers in `lib/ipc.ts`, never with ad hoc inline `invoke()` calls scattered through components.
- Long-running or streaming operations (presence polling, launch progress, asset upload progress) use Tauri's event system (`emit`/`listen`) from a command or background task, rather than the frontend polling a command in a loop.

### 2. Envelope Encryption & Storage Modes

- Accounts and cookies are never stored in plaintext on disk.
- **Envelope Encryption**: The account store (`accounts.dat`) is encrypted with a random 256-bit AES-256-GCM data key. The data key is wrapped in the store header.
- **Store Modes**:
  - `StoreMode::Device` (default): Wrapping key is held in the OS Credential Store (`Windows Credential Manager` via `keyring`). Protects cookies against infostealers while unlocking seamlessly on startup without user interaction.
  - `StoreMode::Password`: Wrapping key is derived via `Argon2id` from a user-supplied master password.
- **Crash-Safe Persistence**: All file writes (`accounts.dat`, `config.json`, presets) must go through `ram_core::storage::atomic_write`, which writes to a sibling `.tmp-*` file, fsyncs (`sync_all`), creates a `.bak` backup, and atomically renames the file into place.

### 3. Instance Tracking & Launch Attribution

- Roblox starts via custom protocol handler (`roblox-player:` URI).
- RM stamps a unique millisecond token (`+launchtime:<millis>+`) into the launch URI.
- The spawned `RobloxPlayerBeta.exe` preserves this string in its command line.
- RM reads the process command line via `OpenProcess` + `ReadProcessMemory` to achieve exact (`Attribution::Exact`) attribution between running PIDs and managed accounts, with fallback to FIFO appearance-order (`Attribution::Inferred`).

### 4. Multi-Instance & Window Management

- **Multi-Instance**: Bypasses Roblox single-instance restriction by inspecting process handles, duplicating the `ROBLOX_singletonEvent` mutex handle, and closing it in both RM and the target process.
- **Window Tiling**: Automatically queries monitor geometry and positions/sizes running Roblox windows into a clean grid layout upon launch.
- **Privacy Mode**: Clears `%LOCALAPPDATA%\Roblox\LocalStorage\RobloxCookies.dat` before launching to prevent Roblox from linking browser cookies to the launcher account.

### 5. Tauri Webview Architecture

- Tauri hosts the React frontend in a single managed WebView2 instance per window - there is no separate subprocess re-exec dance for the main UI.
- For flows that previously required a dedicated WebView2 child process (embedded Roblox login, "browse as" account), evaluate whether a Tauri secondary window (a second `WebviewWindow`) meets the need before reaching for a separate re-exec'd process. A secondary window is simpler and stays inside Tauri's lifecycle management; only fall back to a re-exec'd child process if there's a concrete isolation requirement a secondary window can't satisfy, and document why.
- Regardless of which approach is used, this remains a **security boundary** - see Agent Guidelines § 5.

### 6. Secret Redaction & Logging

- RM writes daily rotating logs (`%APPDATA%\RM\rm.<YYYY-MM-DD>.log`).
- `ram_core::redact::scrub` and `ScrubbingWriter` intercept all log output at write-time, automatically replacing `.ROBLOSECURITY` cookies, `gameinfo:` tickets, CSRF tokens, and Windows username paths with `<redacted>`.
- This applies equally to anything logged from `src-tauri` command handlers - a command that logs its own error context is bound by the same redaction discipline as `ram_core`.

---

## Design System (required read for any UI work)

RM's frontend follows a single documented design system - not per-page
improvisation. Before writing or modifying anything in `ram_ui/src`:

1. Read **`DESIGN.md`** in full.
2. Read **`ram_ui/src/tokens.css`** - the single source of truth for every
   color, spacing, radius, border-width, and motion value in the app.

The short version, expanded fully in that doc:

- **Fixed shell**: activity bar (44px) + sidebar (220px) + content area,
  consistent on every page - an operations-console layout, not a
  marketing-dashboard template. See DESIGN.md § Layout shell.
- **Status color is reserved** for live account/instance/process state
  only - never decoration, never a button.
- **No shadows, one radius, one border weight**, everywhere.
- **Two type families with fixed jobs**: sans for UI chrome, mono for
  anything that's a data value (IDs, timestamps, account names).
- **Semantic tokens only** in component code - never a raw hex, and no
  new component that just wraps a single element for the sake of naming
  it.
- Every interactive component needs its full interaction-state set
  (hover/focus/active/disabled/loading, as applicable) - see
  DESIGN.md § Interaction states. A missing focus ring is a design
  system violation, not a nitpick.
- No emoji as icons. One icon set (`assets/icons`) only.

**Enforcement is intentionally partial** - hard rules (raw hex, box-shadow,
non-standard radius, wrong font-family) fail `stylelint` in CI; softer
judgment calls (an off-scale spacing value with a genuine structural
reason) are a code-review/PR-checklist concern, not a lint failure. See
DESIGN.md § Enforcement for the reasoning - the point is to
constrain design decisions, not turn ordinary development into an
obstacle course. If you hit a case where the linter is fighting a
legitimate structural need (e.g. a fixed-size icon square), that's a sign
to use the literal value and note why in the PR, not to bend the token
scale to fit.

If a task requires a design decision this doc doesn't cover, flag it and
ask rather than inventing a one-off pattern - a new pattern introduced
without discussion is exactly what breaks consistency for the next
contributor.

---

## Development & Build Commands

All commands are run from the workspace root on a Windows host:

```powershell
# Rust: type check the workspace
cargo check

# Rust: run lints (strict warnings matching CI)
cargo clippy --workspace --all-targets -- -D warnings

# Rust: format check (matches CI - run before every commit)
cargo fmt --all -- --check

# Rust: run all unit and integration tests
cargo test --workspace

# Frontend: install dependencies (from ram_ui/)
pnpm install

# Frontend: lint (includes design-token enforcement via stylelint)
pnpm lint

# Frontend: type check
pnpm typecheck

# Run the full app in dev mode (Tauri + Vite dev server, hot reload)
pnpm tauri dev

# Build optimized release bundle (outputs an installer + exe under ram_ui/src-tauri/target/release/)
pnpm tauri build
```

### Pre-commit verification sequence

Run these in order before every commit - this is exactly what CI checks, so a clean local pass means a clean PR:

```powershell
cargo fmt --all -- --check
cargo check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir ram_ui lint
pnpm --dir ram_ui typecheck
```

For UI or launch changes, also manually test on Windows with Roblox installed. Any change touching cookies, encryption, storage, or process control needs explicit mention in the PR description. Any change touching `ram_ui/src` needs explicit confirmation it was checked against `DESIGN.md` (see PR template checklist).

---

## Contribution Workflow (fork-based)

RM is a Windows-only project. Contributions happen through a GitHub fork and pull request - never push directly to `main` on upstream.

### Set up a fork

```powershell
git clone https://github.com/GITHUB_USERNAME/roblox-manager.git
cd roblox-manager
git remote add upstream https://github.com/Paryx-games/roblox-manager.git
git checkout -b your-feature-name
```

Keep `origin` pointed at your fork and `upstream` pointed at RM. Never work directly on `main` for a change you plan to submit.

### Sync before starting new work

```powershell
git fetch upstream
git switch main
git pull --ff-only upstream main
git push origin main
git switch -c your-feature-name
```

If a feature branch already exists, rebase it onto the freshly-synced `main` instead of merging:

```powershell
git switch your-feature-name
git rebase main
```

Resolve conflicts carefully, then re-run the full verification sequence above. Never force-push a shared branch without coordinating with whoever else is on it.

### Commit conventions

Use [Conventional Commits](CONVENTIONAL_COMMITS.md), for example:

```
docs: explain private-server bookmarks
fix(auth): preserve csrf token per cookie
feat(groups): show membership status
feat(ui): add DataTable component per DESIGN.md
```

One focused change per PR. Documentation-only edits can be grouped together if they cover one feature area. Never commit cookies, tokens, logs, build output, `node_modules/`, or local account data - double check the diff before staging.

### Commit as you go - never one giant commit

Commit each logical change as soon as it's done, not once at the end of the task. If a task touches multiple files or concepts, split it into multiple commits along those lines rather than staging everything into a single commit at the finish line. Rough rule of thumb:

- Finished a self-contained piece (one function, one bugfix, one component) → commit it before moving to the next piece.
- About to switch what you're working on within the same task (e.g. done with the backend Tauri command, starting the React component that calls it) → commit first.
- Never batch unrelated changes into one commit just because they happened in the same session.

Run the pre-commit verification sequence above before each commit, not just once at the end - catching a broken intermediate state early is the whole point of committing incrementally.

### Update CHANGELOG.md for user-facing changes

If a commit changes something a user would notice - a new feature, a fixed bug, changed behavior, UI changes - add an entry to `CHANGELOG.md` under an `## Unreleased` heading (create it above the most recent `## vX.Y.Z` heading if it doesn't exist yet) in the same commit as the change itself. Don't wait until release time to backfill it.

Skip the changelog for things a user would never notice: internal refactors, test-only changes, comment/doc tweaks, CI config, dependency bumps with no behavior change.

At release time (see Versioning & Releasing below), the `## Unreleased` heading gets renamed to `## vX.Y.Z` - the entries are already written by then.

### Opening a PR

```powershell
git push -u origin your-feature-name
```

PR description should cover: what changed, why, user-visible behavior, tests run, and any Roblox-version assumptions. Include the design-system checklist items (see DESIGN.md § Enforcement) for any UI change. Report vulnerabilities through [SECURITY.md](SECURITY.md), never as a public issue.

---

## Versioning & Releasing

Full policy lives in [VERSIONING.md](VERSIONING.md); this is the working summary.

### SemVer policy

RM follows Semantic Versioning: `MAJOR.MINOR.PATCH`.

- **MAJOR** - breaking changes requiring migration: incompatible config or account-store formats, removed/renamed dependencies of existing setups, scripts, or integrations. Feature removal alone does _not_ require a major bump.
- **MINOR** - backward-compatible features, settings, tabs, capabilities, or feature removals.
- **PATCH** - bug fixes, wording changes, UI polish. Never adds capability.

RM has no project-wide `0.x`/beta phase - every version line ships stable as `MAJOR.MINOR.PATCH`. Pre-release identifiers stage a version before its plain tag:

- `-alpha.N` - early, unfinished build; expect breakage.
- `-beta.N` - early test build; may lack features or contain bugs.
- `-rc.N` - release candidate; if clean, publish the plain version next. An `-rc.N` requires an earlier `-alpha.N` or `-beta.N` for that version, otherwise just publish the plain version directly.

Pre-releases sort before their plain release under SemVer (`v2.0.0-rc.1` < `v2.0.0`) - tooling should never treat a pre-release as latest stable.

**Version lives only in the root `Cargo.toml`.** Both crates inherit it, and the Tauri app's `tauri.conf.json` version field must match it - never hardcode a version anywhere else, including `package.json`.

### Publishing a release

`.github/workflows/release.yml` fires only on a pushed tag matching `v*` - normal pushes to `main` never publish.

1. Bump the version in the root `Cargo.toml` (and `tauri.conf.json` / `package.json` to match).
2. Rename the `## Unreleased` heading in `CHANGELOG.md` to `## vX.Y.Z` (entries should already be there from per-commit updates - see Commit as you go above). If for some reason there's no `## Unreleased` section, add `## vX.Y.Z` above the previous release instead. **The workflow fails if it can't find a heading matching the tag exactly.**
3. If dependencies changed, sync the lockfiles and check the diffs are expected:

   ```powershell
   cargo build
   git diff Cargo.lock
   pnpm --dir ram_ui install
   git diff ram_ui/pnpm-lock.yaml
   ```

   Both `Cargo.lock` and `pnpm-lock.yaml` must be included in the release commit - the workflow runs with `--locked`/`pnpm install --frozen-lockfile` and fails on a stale lockfile.

4. Commit the release files:

   ```powershell
   git add Cargo.toml Cargo.lock CHANGELOG.md ram_ui/src-tauri/tauri.conf.json ram_ui/package.json ram_ui/pnpm-lock.yaml
   git commit -m "chore: bump version to vX.Y.Z"
   ```

5. Tag and push:

   ```powershell
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

6. The workflow builds and publishes automatically - renames the installer/exe to match `roblox-manager-vX.Y.Z-windows-x64`, and adds changelog content, GitHub-generated notes, a downloads table, and a SHA256 checksum.

**If the tag needs to move** (fix landed in a newer commit, workflow has no `workflow_dispatch` trigger so this is the only way to re-trigger):

```powershell
git tag -d vX.Y.Z
git push origin :refs/tags/vX.Y.Z
git tag vX.Y.Z
git push origin vX.Y.Z
```

Only move a tag before anyone relies on it - never once users have downloaded the build or automation pins it. If the tagged commit itself is fine and it just failed transiently, rerun from **Actions → Re-run all jobs** instead of moving the tag.

---

## Agent Guidelines & Coding Standards

1. **Windows Platform Assumptions**:
   - The application is Windows-specific (`windows-sys`, Win32 API calls, Windows Credential Manager, `%APPDATA%\RM`, `%LOCALAPPDATA%\Roblox`).
   - Use `PathBuf` and handle path resolution cleanly without hardcoding Unix-only assumptions. Do not add cross-platform code paths unless explicitly asked.

2. **UI & Thread Safety**:
   - Never perform blocking network requests or heavy disk I/O directly inside a `#[tauri::command]` handler on the main thread - use `tokio::spawn` / async command handlers, and emit events for progress on long-running operations rather than blocking the caller.
   - On the frontend, never block React's render path on a synchronous IPC call - all `invoke()` calls in `lib/ipc.ts` are async and components should handle their pending/error states explicitly (see DESIGN.md § Interaction states for what "loading" must look like).

3. **Editing large or unfamiliar files**:
   - Prefer targeted, context-anchored edits (matching on surrounding unique text) over line-number-based deletion or full-file rewrites. Line numbers shift, get miscounted, or go stale between reading a file and editing it - a slice-by-line-range script is exactly how code gets silently dropped or orphaned without either the editor or the compiler noticing.
   - After any large deletion, insertion, or rewrite, re-read the affected region of the file before moving on. Don't assume an edit landed as intended just because the tool call returned success - confirm the content is actually there, especially anything that isn't guaranteed to trip a compile/type error if missing (UI sections, string literals, whole blocks that are merely absent rather than syntactically broken).
   - A clean `cargo check`/`pnpm typecheck` after an edit means the code is _syntactically valid_, not that the intended change is _present_. Missing UI, a dropped feature, or a silently-vanished section will not show up as a type error - verify the actual diff, not just the exit code.
   - If a file is large enough that in-place editing feels risky or a rewrite is genuinely warranted, say so and confirm the approach before doing a full-file replacement, rather than declaring intent to do one ("the file is quite large, let me create a complete replacement") and then continuing with the same risky line-surgery anyway.
   - Don't re-run a command you've already gotten a clean or informative result from on a hunch - e.g. running `cargo fmt --all` after `cargo fmt --all -- --check` already passed clean is redundant work chasing a discrepancy that should be investigated, not brute-forced.
   - If a shell command fails because it's the wrong tool for the shell (e.g. `head`/`tail` on PowerShell), fix it once and adjust for the rest of the session - don't repeat the same category of shell mistake across multiple commands.

4. **Data Integrity & Persistence**:
   - Always use `ram_core::storage::atomic_write` or `atomic_swap` when persisting config, presets, or account stores.
   - Do not use bare `std::fs::write` on persisted application state files - this risks corruption on crash or power loss.
   - This applies from `src-tauri` command handlers too - a command that persists state must go through `ram_core::storage`, not write directly.

5. **🔐 Security & Secrets** (read this one carefully - RM's entire value proposition is not leaking credentials):
   - Never log `.ROBLOSECURITY` cookies, session tickets, master passwords, CSRF tokens, or other credentials in plaintext.
   - Never include cookies, tokens, passwords, authentication headers, or other secrets in user-facing errors, debug output, panic messages, or `tracing` fields - an `Err(CoreError::Auth(format!("request failed: {cookie}")))`-shaped bug is a credential leak, not a convenience. Error variants that carry request context must carry an identifier (account alias, request ID), never the secret itself. This applies equally to errors surfaced from `src-tauri` commands to the frontend via `Result`/events - a secret that never touched a log file but did get serialized into a Tauri IPC error payload is still a leak.
   - Never include authentication cookies, tokens, passwords, or other secrets in URLs, query parameters, logs, telemetry, or crash/analytics data.
   - Avoid cloning, storing, or passing secret values beyond the scope required to perform the operation - the smaller the blast radius of a value's lifetime, the fewer places it can leak from. Prefer passing references or scoping a secret to the function that needs it over threading it through unrelated layers. This includes not passing raw secrets across the Tauri IPC boundary into the frontend at all where the frontend only needs to know _that_ an account is authenticated, not the cookie itself.
   - Do not weaken, bypass, or remove existing encryption, credential-store, redaction, or privacy protections unless the task explicitly requires it - and if it does, call that out clearly rather than doing it quietly as a side effect of something else.
   - Treat all external API responses (Roblox REST/Open Cloud), user input, imported presets/config, and other process data as untrusted. Validate before using it in security-sensitive operations, filesystem paths, process arguments, or network requests - this includes preset files, group/asset API responses, and anything read from `ReadProcessMemory`. This also includes anything the frontend sends to a Tauri command - the frontend is not a trusted boundary just because it's "our own UI."
   - Never commit credentials, cookies, tokens, account data, logs, memory dumps, or other sensitive local data - this applies to test fixtures too; use synthetic values, never a real captured cookie or ticket, even redacted.
   - Do not replace security-sensitive crates or cryptographic primitives (`aes-gcm`, `argon2`, `sha2`, `rand`, `keyring`) with alternatives, and do not hand-roll crypto, without explicit approval. "This crate is annoying to work with" is not a reason to swap the encryption or credential-storage backend.
   - When introducing new secret patterns or URI parameters, update `ram_core::redact` rules in the same change - a new secret type with no matching redaction rule is a silent leak waiting to happen.
   - The Tauri webview boundary (§ Architecture 5) is a security boundary, not just a technical detail - anything that flows from a login/browse-as window (auth tickets, cookies) must go through the same redaction/storage discipline as everything else, not be treated as "already handled" because it came from a separate window or process.
   - Launch-attribution tokens (`+launchtime:<millis>+`) and other data read via `OpenProcess`/`ReadProcessMemory` are untrusted process data - validate shape before using it, and never let it flow into logs unredacted.
   - If a feature ever copies a credential, cookie, token, or access code to the system clipboard (e.g. a "copy access code" button), that's a deliberate, explicit action the user triggered - never place secrets on the clipboard as a side effect of some other operation, and prefer copying the least-sensitive identifier that still does the job (e.g. an account alias over a raw cookie) where the feature allows it.

6. **Error Handling**:
   - Use `ram_core::error::CoreError` with `thiserror` in `ram_core`.
   - `src-tauri` commands convert `CoreError` into a serializable error type at the IPC boundary rather than leaking internal error variants directly - see § 5 on not letting secrets ride along in that conversion.
   - On the frontend, surface errors via the UI's `<ErrorState>` / toast pattern (see DESIGN.md), not a raw thrown-error or console-only failure.

7. **Code comments**:
   - Comments explain intent, not syntax. Lowercase, concise, and only where the "why" isn't obvious from the code itself. No redundant or obvious comments.

8. **Don't create unrequested files** (especially `.md` summary/report files):
   - Don't create summary, report, or "complete"-style `.md` files (e.g. `SETTINGS_REFACTOR_COMPLETE.md`) to document what was done in a task. A task summary belongs in the chat/PR response and, if the change is user-facing, in the `CHANGELOG.md` entry - not as a new standalone file sitting in the repo.
   - The commit message and PR description are the record of what changed and why. A separate markdown write-up duplicates that with no reader - nobody re-opens `FOO_REFACTOR_COMPLETE.md` later; they read `git log` or the changelog.
   - Only create a new file when the task explicitly calls for one (a real doc page, a new source file, a test file for new functionality) or when the user asks for a written summary as a deliverable. When in doubt, don't create it - say the summary in the response instead.
   - Before finishing a task, check for any stray files you created along the way (scratch notes, temp scripts, "plan" files) that were only useful during the task itself, and remove them rather than leaving them in the repo.

9. **GUI consistency**: reuse existing components from `ram_ui/src/components/` and tokens from `ram_ui/src/tokens.css` wherever possible. Do not introduce a separate visual style, a new one-off component, or redesign an existing UI pattern without a clear reason - and never without reading `DESIGN.md` first. See § 10.

10. **Design system compliance is mandatory for any UI change**:
    - Read `DESIGN.md` and `ram_ui/src/tokens.css` before writing or editing anything under `ram_ui/src`. This is a required read in the same sense the docx/pptx skills are required reads before touching those file types - skipping it produces exactly the generic soft-shadow-rounded-card-with-emoji-icons output the doc's Anti-patterns section exists to prevent.
    - Never introduce a raw hex color, a `box-shadow`, a non-standard `border-radius`, or a `font-family` outside `tokens.css` - these fail CI via `stylelint` (see DESIGN.md § Enforcement), but don't rely on the linter to catch what review should catch first.
    - Every new interactive component must implement its full required interaction-state set (DESIGN.md § Interaction states) - a button with no visible focus ring is an incomplete component, not a follow-up task.
    - Don't invent a new component for a single-use wrapper (DESIGN.md § 4) - and don't invent a new _pattern_ (a new card style, a new table variant) without flagging it and confirming the approach first, the same way a major architectural change gets flagged under § "Ask before major changes" below.

---

## Notes

<!-- Add personal notes, development reminders, feature ideas, and custom workflows below -->

- **Changelog & versioning:** Add a `CHANGELOG.md` entry under `## Unreleased` in the same commit as any user-facing change (see Update CHANGELOG.md under Contribution Workflow) - don't leave it for release time. Keep versioning organised; group related changes into appropriate releases rather than unnecessarily cramming unrelated changes into a single version.
- **Git safety:** Commit completed changes as you go, in small logical chunks - not as one large commit at the end (see Commit as you go under Contribution Workflow). Never modify git configuration or remotes, change the origin repository, force-push, reset or discard unrelated work, or perform other destructive git operations.
- **Review before committing:** Before creating a commit, review the diff and ensure that all staged changes are relevant to the requested task. Do not commit unrelated or accidental changes - this includes stray summary/report `.md` files (see Agent Guidelines § 8).
- **Preserve existing behaviour:** Avoid changing existing functionality unless the task explicitly requires it. Prefer small, targeted changes over unnecessary refactors.
- **Ask before major changes:** If a requested change would require a significant architectural change, removal of existing functionality, a new design-system pattern not already covered by `DESIGN.md`, or a potentially destructive migration, explain the impact before proceeding.
- Follow the repository's [Conventional Commits guide](CONVENTIONAL_COMMITS.md) for commit messages, and [VERSIONING.md](VERSIONING.md) for how version numbers are chosen.
- Full user-facing docs (guides, FAQ, security guidance) live at `https://roblox-manager.gitbook.io/docs` - check there before re-explaining a feature that's already documented for users.
- All icons use the existing set under `assets/icons` (.pngs OR .svgs ONLY) - see DESIGN.md § Icons for the full rule, including the no-emoji rule.
