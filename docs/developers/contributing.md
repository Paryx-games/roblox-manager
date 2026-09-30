---
description: Set up a fork, sync it, make focused commits, and open a PR.
icon: code-branch
---

# Contributing

RM is a Windows-only Rust application with a Tauri shell and React/TypeScript frontend. Contributions are made through a GitHub fork and pull request.

## Prerequisites and source layout

Use stable Rust, Node 22, pnpm 11, Visual Studio C++ Build Tools, a Windows SDK and WebView2. Roblox is needed for launch testing.

- `ram_core/`: existing headless logic, encryption, storage, Roblox APIs and process management.
- `ram_ui/src-tauri/`: active desktop commands, windows, events, background work, logging and installer files.
- `ram_ui/frontend/`: active React pages, shared controls, tokens and typed IPC wrappers.

The active application is React + Tauri with reusable domain behavior in `ram_core`. The v2 branch includes narrowly scoped, behavior-preserving extractions from Tauri into core. Follow the limits in [AGENTS.md](https://github.com/Paryx-games/roblox-manager/blob/v2/AGENTS.md) before changing core. Browser subprocess and Windows startup helpers live in `ram_ui/src-tauri/src/`. Read [DESIGN.md](https://github.com/Paryx-games/roblox-manager/blob/v2/DESIGN.md) and `ram_ui/frontend/tokens.css` before interface work. Never expose stored credentials through frontend responses or diagnostics.

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

## Open a pull request

```powershell
git push -u origin dev/your-feature-name
```

Follow [PR_CONVENTIONS.md](https://github.com/Paryx-games/roblox-manager/blob/v2/PR_CONVENTIONS.md), including all required description sections and checklist items. Add user-facing changes to the unreleased changelog, respecting the v2 page-completion exception. Open a PR from your fork and describe what changed, why, user-visible behavior, tests run, and any Roblox-version assumptions. Use [SECURITY.md](https://github.com/Paryx-games/roblox-manager/blob/v2/SECURITY.md) for vulnerabilities instead of a public issue.
