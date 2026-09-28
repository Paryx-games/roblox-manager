# Contributing to RM

Thanks for wanting to work on RM. This project touches Roblox authentication cookies and Windows process control, so contributions need a bit more care than a typical UI project. The workflow is a standard GitHub fork-and-PR.

## Before you start

- Check open [Issues](https://github.com/Paryx-games/roblox-manager/issues) and [Pull Requests](https://github.com/Paryx-games/roblox-manager/pulls) so you're not duplicating work.
- For anything non-trivial (new feature, behavior change, anything touching encryption/process control), open an issue first to discuss the approach before writing code. Small fixes and obvious bugs don't need this.
- RM is Windows-only right now. Cross-platform support is out of scope unless explicitly discussed in an issue first; don't submit speculative portability branches such as `#[cfg(unix)]`.

## Project layout

```text
ram_core/   Headless models, encryption, storage, Roblox APIs, process control, and tests
ram_ui/frontend/   Active React/TypeScript interface, shared components and typed IPC
ram_ui/src/tokens.css Shared visual tokens imported by the active frontend
ram_ui/src-tauri/  Tauri commands, windows, background tasks, logging and NSIS packaging
ram_ui/src/        Retained legacy egui application and shared token stylesheet
assets/     Bundled application assets
```

`ram_core` has no UI dependency. Its reusable domain logic and Roblox API operations live there; Tauri owns desktop state, windows, IPC and event coordination. The v2 branch includes explicitly requested extractions from Tauri into core. Keep future core edits within the limited policy in [AGENTS.md](AGENTS.md). The egui application remains in `ram_ui/src/`; this branch also contains a small login compatibility helper there. React uses typed wrappers in `ram_ui/frontend/lib/ipc.ts`. Read [DESIGN.md](DESIGN.md) and `ram_ui/src/tokens.css` before interface edits.

## Getting set up

### Prerequisites

- Windows 10 or Windows 11
- [Rust](https://rustup.rs/) stable
- Node.js 22 and pnpm 11, matching CI
- Visual Studio Build Tools with Desktop development with C++ and a Windows SDK
- Roblox installed for local launch testing
- WebView2 installed for browser-login and browse-as windows

### Fork and branch

1. Click **Fork** on this repo's GitHub page to create your own copy under your GitHub account.
2. Clone *your fork* (not this repo) to your computer, replacing `GITHUB_USERNAME` below with your actual GitHub username:

```powershell
git clone https://github.com/GITHUB_USERNAME/roblox-manager.git
cd roblox-manager
git config core.hooksPath .githooks
git remote add upstream https://github.com/Paryx-games/roblox-manager.git
git checkout -b dev/your-feature-name
pnpm --dir ram_ui install --frozen-lockfile
```

Configure the repository hooks immediately after cloning so the project's commit and push checks run automatically.

Branch off the default branch. Keep branch names short and descriptive.

Start the desktop app with `pnpm --dir ram_ui tauri dev`. For an independent debug executable, use `pnpm --dir ram_ui build:debug`, then `target/debug/rm_tauri.exe`. Production packaging uses `pnpm --dir ram_ui tauri build --ci --bundles nsis -- --locked` and outputs the executable and NSIS installer under `target/release/`.

## Making changes

- Keep PRs focused - one fix or feature per PR. Don't bundle unrelated cleanup in with a feature change; open a separate PR for that.
- Follow the repository's [Conventional Commits guide](CONVENTIONAL_COMMITS.md) for commit messages (`feat:`, `fix:`, `refactor:`, `docs:`, etc), and [VERSIONING.md](VERSIONING.md) for how version numbers are chosen.
- Comments should explain *why*, not *what* - if the code already makes the "what" obvious, skip the comment.
- If you're touching cookie handling, encryption, storage, or process control (mutex patching, client attribution, etc), say so explicitly in your PR description. These get closer review than UI-only changes.

## Before opening a PR

Run these checks locally before pushing:

```powershell
cargo fmt --all -- --check
cargo check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir ram_ui lint
pnpm --dir ram_ui typecheck
```

A PR should pass Rust and frontend checks with no warnings. GitHub Actions builds the React frontend before the Rust app and verifies release packaging separately. Run the sequence before each focused commit, review the diff, and keep generated files and private data out of it.

For data, recovery or migration testing, use a separate test setup and backed-up test stores. Never run start-over or recovery tests against your only store. For installer changes, test on a separate Windows user or VM with fresh install, upgrade, shortcut, cleanup and missing-WebView2 cases. A successful bundle build is not an interactive smoke test.

## Opening the PR

Follow [PR_CONVENTIONS.md](PR_CONVENTIONS.md) for the required title, description sections and checklist. Add user-facing changes to `CHANGELOG.md` under `Unreleased`, respecting the v2 page-completion exception in AGENTS.md. Documentation-only changes do not need a changelog entry.

- Describe *what* changed and *why*, not just a restatement of the diff.
- Link the issue it resolves, if any (`Fixes #123`).
- If it's a behavior change a user would notice, mention it - this feeds into release notes.
- Screenshots or a short clip for UI changes are appreciated but not required.
- Link [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) or [SECURITY.md](SECURITY.md) when those policies are relevant to the contribution.

## What review looks like

- Expect questions, especially for anything touching cookies, encryption, or Roblox client/process handling - this isn't personal, that's just where mistakes are most expensive.
- Small logic or style requests may get pushed as suggestions you can apply directly.
- PRs that go quiet for a while may get closed to keep the queue clean - feel free to reopen or ping if you're still working on it.

## Reporting bugs vs. reporting vulnerabilities

Regular bugs (crashes, UI issues, launch failures, etc) go in [Issues](https://github.com/Paryx-games/roblox-manager/issues) as normal.

**Do not** open a public issue for anything that could expose cookies, bypass encryption, corrupt protected account data, or otherwise compromise an account. See [SECURITY.md](SECURITY.md) for how to report those privately.

## A note on scope

RM patches Roblox's singleton mutex and reads Roblox's local files, process command lines, and launcher protocol. None of these are documented or guaranteed by Roblox. If your contribution depends on undocumented behavior, flag that clearly in the PR so it can be tracked as something that might silently break after a Roblox update.
