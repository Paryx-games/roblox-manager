# Contributing to RM

Thanks for wanting to work on RM. This project touches Roblox authentication cookies and Windows process control, so contributions need a bit more care than a typical UI project. The workflow is a standard GitHub fork-and-PR.

## Before you start

- Check open [Issues](../../issues) and [Pull Requests](../../pulls) so you're not duplicating work.
- For anything non-trivial (new feature, behavior change, anything touching encryption/process control), open an issue first to discuss the approach before writing code. Small fixes and obvious bugs don't need this.
- RM is Windows-only right now. Cross-platform support is out of scope unless explicitly discussed in an issue first; don't submit speculative portability branches such as `#[cfg(unix)]`.

## Project layout

```text
ram_core/   Headless models, encryption, storage, Roblox APIs, process control, and tests
ram_ui/     Native egui application, async bridge, browser subprocesses, and UI components
assets/     Bundled application assets
```

`ram_core` has no UI dependency and should stay that way. Logic that doesn't need egui belongs in `ram_core`, not `ram_ui`. The `ram_ui` render loop must stay responsive: networking, storage, authentication, and process work goes through the Tokio-backed backend bridge rather than running synchronously in `update` or a component renderer.

## Getting set up

### Prerequisites

- Windows 10 or Windows 11
- [Rust](https://rustup.rs/) stable, 1.75+
- Roblox installed for local launch testing
- WebView2 installed for browser-login and browse-as windows

### Fork and branch

1. Click **Fork** on this repo's GitHub page — this creates your own copy under your GitHub account.
2. Clone *your fork* (not this repo) to your computer, replacing `GITHUB_USERNAME` below with your actual GitHub username:

```powershell
git clone https://github.com/GITHUB_USERNAME/roblox-manager.git
cd roblox-manager
git checkout -b your-feature-name
```

Branch off the default branch. Keep branch names short and descriptive.

## Making changes

- Keep PRs focused - one fix or feature per PR. Don't bundle unrelated cleanup in with a feature change; open a separate PR for that.
- Follow the repository's [Conventional Commits guide](CONVENTIONAL_COMMITS.md) for commit messages (`feat:`, `fix:`, `refactor:`, `docs:`, etc).
- Comments should explain *why*, not *what* - if the code already makes the "what" obvious, skip the comment.
- If you're touching cookie handling, encryption, storage, or process control (mutex patching, client attribution, etc), say so explicitly in your PR description. These get closer review than UI-only changes.

## Before opening a PR

Run these checks locally before pushing:

```powershell
cargo check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

A PR should build, pass the workspace tests, and have no clippy warnings. The repository also contains GitLab CI configuration, but local checks are still required before opening a PR.

If you changed anything that reads or writes account data, config, or presets, manually verify a fresh `%APPDATA%\RM` still works (or that migration from an existing store doesn't corrupt it).

## Opening the PR

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

Regular bugs (crashes, UI issues, launch failures, etc) go in [Issues](../../issues) as normal.

**Do not** open a public issue for anything that could expose cookies, bypass encryption, corrupt protected account data, or otherwise compromise an account. See [SECURITY.md](SECURITY.md) for how to report those privately.

## A note on scope

RM patches Roblox's singleton mutex and reads Roblox's local files, process command lines, and launcher protocol. None of these are documented or guaranteed by Roblox. If your contribution depends on undocumented behavior, flag that clearly in the PR so it can be tracked as something that might silently break after a Roblox update.
