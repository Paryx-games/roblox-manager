# PR_CONVENTIONS.md - Pull Request Rules for RM (Roblox Manager)

This file is the authoritative spec for how pull requests are titled,
structured, and checklisted in this repo. See [AGENTS.md](AGENTS.md) for
the rest of the contribution workflow (forking, commits, versioning,
design system, security rules).

**Title**: conventional commit style summarizing the overall change, e.g. `feat(combat): add knockback system` - same type/scope/description rules as commit messages (see [CONVENTIONAL_COMMITS.md](CONVENTIONAL_COMMITS.md)). If the PR spans multiple scopes and no single one is honest, omit the scope rather than picking one that undersells the diff.

**Description** MUST include, in this order, and never skip a section:

1. **What changed and why** - the actual change and the reasoning behind it, not a restatement of the title. Link related issues (`closes #NN`).
2. **User-visible behavior** - what a user would notice, if anything. If nothing changes for users (internal refactor, test-only, tooling), say so explicitly rather than leaving the section blank.
3. **Gotchas** - anything a reviewer needs to watch out for: risky edge cases, non-obvious tradeoffs, follow-up work left undone, Roblox-version assumptions baked into the change.
4. **How to test** - concrete steps to verify the change, not "tested locally." For UI or launch changes this means manual Windows steps with Roblox installed, not just `cargo test`/`pnpm test` output.

Keep it concise. No filler, no "This PR introduces..." or similar opener - start directly with the content.

## Required checklist items

State each of these explicitly (checked or N/A) in the description - don't just imply them:

- [ ] Ran the full pre-commit verification sequence clean: `cargo fmt --all -- --check`, `cargo check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm --dir ram_ui lint`, `pnpm --dir ram_ui typecheck`
- [ ] `CHANGELOG.md` updated under `## Unreleased` if this is user-facing (N/A otherwise)
- [ ] If this touches `ram_ui/src`: confirmed against `DESIGN.md` and `tokens.css` - no raw hex/box-shadow/off-token radius, full interaction-state set on new interactive components
- [ ] If this touches cookies, encryption, storage, or process control: explicitly called out in the description, not left implicit
- [ ] If this touches `ram_core::redact`: new secret/URI patterns have matching redaction rules
- [ ] No stray files left in the diff (scratch notes, summary `.md` files, temp scripts)
- [ ] Diff reviewed end-to-end - nothing unrelated to the stated change is staged, no cookies/tokens/account data/logs
- [ ] Neither `ram_core` nor the existing egui source was touched, unless the task explicitly called for it

Report vulnerabilities through [SECURITY.md](SECURITY.md), never as a public issue or in a PR description.

## One focused change per PR

If the checklist above is hard to fill in cleanly because the PR is doing several unrelated things, that's a signal to split it, not to write a longer description.
