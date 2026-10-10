---
description: Set up a fork, sync it, make focused commits, and open a PR.
icon: code-branch
---

# Contributing

RM is a Windows-only Rust application with a Tauri shell and React/TypeScript frontend. Contributions are made through a GitHub fork and pull request.

## Prerequisites and source layout

Use stable Rust, Node 22, the pnpm version pinned in the root `package.json` (currently 12.10.1), Visual Studio C++ Build Tools, a Windows SDK and WebView2. Roblox is needed for launch testing.

- `ram_core/`: existing headless logic, encryption, storage, Roblox APIs and process management.
- `ram_ui/src-tauri/`: active desktop commands, windows, events, background work, logging and installer files.
- `ram_ui/frontend/`: active React pages, shared controls, tokens and typed IPC wrappers.

The active application is React + Tauri with reusable domain behavior in `ram_core`. Follow the limits in [AGENTS.md](https://github.com/Paryx-games/roblox-manager/blob/main/AGENTS.md) before changing core. Browser subprocess and Windows startup helpers live in `ram_ui/src-tauri/src/`. Read [DESIGN.md](https://github.com/Paryx-games/roblox-manager/blob/main/DESIGN.md) and `ram_ui/frontend/tokens.css` before interface work. Never expose stored credentials through frontend responses or diagnostics.

## Set up a fork

```powershell
git clone https://github.com/GITHUB_USERNAME/roblox-manager.git
cd roblox-manager
git config core.hooksPath .githooks
git remote add upstream https://github.com/Paryx-games/roblox-manager.git
git checkout -b dev/your-feature-name
```

Keep `origin` pointed at your fork and `upstream` pointed at RM. Do not work directly on `main` for a change you plan to submit.

Install with `pnpm --dir ram_ui install --frozen-lockfile`, then run `pnpm --dir ram_ui tauri dev` for hot reload. `pnpm --dir ram_ui build:debug` produces a standalone `target/debug/rm_tauri.exe` with the frontend embedded. A dev-server executable needs Vite running; do not confuse it with the standalone debug build.

## Screenshot demo builds

Run `pnpm --dir ram_ui build:debug --demo` to build a standalone demo executable at `target/debug/rm_tauri.exe`. Alternatively, use `pnpm --dir ram_ui tauri dev --demo` for hot reload. A normal debug executable also accepts `--demo` at startup.

Demo builds always open in demo mode, with local Builderman and Roblox avatars, synthetic accounts, and sample workspaces. They skip normal account/config loading, credential access, file logging, background tasks, and update checks. A separate WebView2 profile keeps browser storage isolated. All commands outside the sample-data allowlist are rejected, including account changes, login, launch, process control, uploads, and settings persistence.

`--demo` is rejected for release builds and custom profiles. Rust also refuses to compile a demo-enabled binary without debug assertions. To return to a normal debug executable, rebuild without `--demo`.

## Sync before starting

```powershell
git fetch upstream
git switch main
git pull --ff-only upstream main
git push origin main
git switch -c dev/your-feature-name
```

If your feature branch already exists, rebase it after updating `main`:

```powershell
git switch dev/your-feature-name
git rebase main
```

Resolve conflicts carefully, then run the checks below. Do not force-push a shared branch without coordinating with its users.

## Make focused commits

Use Conventional Commits, for example:

```text
docs: explain private-server bookmarks
fix(auth): preserve csrf token per cookie
feat(groups): show membership status
```

Keep each commit coherent and avoid committing cookies, tokens, logs, build output, or local account data. One focused change per pull request is preferred; documentation-only edits can be grouped when they describe one feature area.

## Verify locally

```powershell
cargo fmt --all -- --check
cargo check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir ram_ui lint
pnpm --dir ram_ui typecheck
```

Run this sequence before every commit. For UI or launch changes, also test on Windows with Roblox installed. Changes involving cookies, encryption, storage, or process control need explicit mention in the pull request.

Use separate test data and backups for migrations, start-over and recovery. Do not test against your only account store. Installer testing belongs on a separate Windows user or VM; see [Releasing RM](releasing.md#installer-verification).

## CI checks and failure comments

The `CI` workflow selects checks from changed file paths, using the same classification code as the PR label job. Labels do not control which checks run. Documentation-only PRs skip frontend checks, Rust checks and Windows packaging. Frontend changes run frontend checks and Windows packaging; Rust changes run Rust checks and Windows packaging. Shared or unknown build inputs, incomplete file lists, and changes to the build/selection workflow run the full set. Pushes to `main`/`v2` and manual runs also run the full set.

Workflow syntax checks, CI script tests and online Zizmor audits run on every PR. The **CI passed** check validates every selected job and rejects failures, cancellations and unexpected skips. Repository maintainers should require this check in branch protection instead of requiring each conditional job individually. Older PR runs are cancelled when superseded; running push and manual validations are allowed to finish.

When PR CI fails, the **CI failure reporter** creates one comment with failed jobs, failed steps and log links. It checks progress every 20 seconds for up to one hour, appends new failures to the same comment, and moves recovered jobs into a **Fixed** dropdown with checked items. A completion event supplies the final update. Once the latest run passes, the reporter hides the comment as **Resolved**; a later failure starts a new comment. Raw logs are kept in Actions rather than copied into PR comments.

The reporter uses API data and reporting scripts from the trusted default branch, so fork PRs do not receive write credentials. It ignores closed PRs, stale commits and superseded runs. The reporter must be present on the repository's default branch before it can run. GitHub moderation/API errors fail the reporter job and are retried by the next reporter invocation; CI results remain available in the Checks tab.

To validate workflow changes locally, run:

```powershell
node --test .github/scripts/*.test.cjs
actionlint
zizmor .github
```

Zizmor requires an available `GH_TOKEN`, `GITHUB_TOKEN` or `ZIZMOR_GITHUB_TOKEN` for online audits. CI supplies its built-in token. The workflow installs pinned Zizmor and Actionlint versions; Actionlint's binary is also SHA256-verified. Intentional Zizmor exceptions have reasons beside their ignore comments.

## Open a pull request

```powershell
git push -u origin dev/your-feature-name
```

Follow [PR_CONVENTIONS.md](https://github.com/Paryx-games/roblox-manager/blob/main/PR_CONVENTIONS.md), including all required description sections and checklist items. Add user-facing changes to the pending version section in the changelog (v2.2.0 while unpublished), respecting the page-completion exception in AGENTS.md. Open a PR from your fork and describe what changed, why, user-visible behavior, tests run, and any Roblox-version assumptions. Use [SECURITY.md](https://github.com/Paryx-games/roblox-manager/blob/main/SECURITY.md) for vulnerabilities instead of a public issue.
