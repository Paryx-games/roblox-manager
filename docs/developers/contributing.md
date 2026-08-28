---
description: Set up a fork, sync it, make focused commits, and open a PR.
icon: code-branch
---

# Contributing

RM is a Windows-only Rust project. Contributions are made through a GitHub fork and pull request.

## Set up a fork

```powershell
git clone https://github.com/GITHUB_USERNAME/roblox-manager.git
cd roblox-manager
git remote add upstream https://github.com/Paryx-games/roblox-manager.git
git checkout -b your-feature-name
```

Keep `origin` pointed at your fork and `upstream` pointed at RM. Do not work directly on `main` for a change you plan to submit.

## Sync before starting

```powershell
git fetch upstream
git switch main
git pull --ff-only upstream main
git push origin main
git switch -c your-feature-name
```

If your feature branch already exists, rebase it after updating `main`:

```powershell
git switch your-feature-name
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
```

For UI or launch changes, also test on Windows with Roblox installed. Changes involving cookies, encryption, storage, or process control need explicit mention in the pull request.

## Open a pull request

```powershell
git push -u origin your-feature-name
```

Open a PR from your fork and describe what changed, why, user-visible behavior, tests run, and any Roblox-version assumptions. Use [SECURITY.md](../../SECURITY.md) for vulnerabilities instead of a public issue.