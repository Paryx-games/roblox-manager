# PR_CONVENTIONS.md - Pull Request Rules for RM (Roblox Manager)

This file is the authoritative spec for how pull requests are titled,
structured, and checklisted in this repo. See [AGENTS.md](AGENTS.md) for
the rest of the contribution workflow (forking, commits, versioning,
design system, security rules).

**Title**: conventional commit style summarizing the overall change, e.g. `feat(combat): add knockback system` - same type/scope/description rules as commit messages (see [CONVENTIONAL_COMMITS.md](CONVENTIONAL_COMMITS.md)). If the PR spans multiple scopes and no single one is honest, omit the scope rather than picking one that undersells the diff.

## Description

Use the repository template as a flexible starting point. **Changes**,
**Testing**, and **Checklist** are required, in that order; optional sections
can be removed or adapted.

- **Changes**: Explain the concrete change in a short paragraph or bullets.
  Include user-visible behavior when relevant, the reason when it helps
  explain the change, and related issues (`Closes #NN`) when applicable.
- **Testing**: List relevant checks run and results. Choose checks based on
  the change's scope and risk. Include concrete manual steps for UI or launch
  changes on Windows with Roblox installed. Explain relevant failures or
  outstanding verification rather than claiming a clean pass.
- **Notes (optional)**: Record relevant limitations, compatibility assumptions,
  security-sensitive changes, or follow-up work. Delete when unnecessary.
- **Screenshots (optional)**: Add visuals when they help explain interface
  changes. Delete when unnecessary; no N/A statement is required.

Keep it concise. No filler, no "This PR introduces..." or similar opener - start directly with the content.

## Required checklist

Complete every checklist item for every PR; do not leave copied defaults
unreviewed. Check completed items, replace the checkbox with `N/A` when an item
does not apply, and leave an item unchecked only when it is outstanding and
the gap is explained in Testing. The template is flexible: adapt wording to
the change, while retaining relevant coverage of these four areas.

- [ ] Relevant checks and results are recorded in Testing; applicable omissions or failures are explained.
- [ ] Changelog updated for user-visible changes (N/A if not applicable).
- [ ] Applicable design, security, and core ownership requirements reviewed (N/A if none apply).
- [ ] Diff reviewed for unrelated files, secrets, local data, and build artifacts.

Full local verification is recommended for broad, high-risk, or cross-cutting
changes. For smaller changes, run the smallest existing checks that cover the
diff and explain relevant gaps in Testing. The full verification command set is:

- Full verification means `cargo fmt --all -- --check`, `cargo check`,
  `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `pnpm --dir ram_ui lint`, and `pnpm --dir ram_ui typecheck`.
- User-facing changes need a `CHANGELOG.md` entry under the current unpublished version heading (v2.2.0 while pending), or `## Unreleased` after that version is published,
  subject to AGENTS.md's v2 page rewrite exception.
- Frontend changes need explicit confirmation against `DESIGN.md` and
  `tokens.css`, plus applicable visual, keyboard, state, scaling, clipping,
  and reduced-motion checks in Testing. No separate interface checklist is needed.
- Changes touching cookies, encryption, storage, or process control must be
  called out in Changes or Notes. Preserve atomic persistence, IPC validation,
  and matching redaction for new secret/URI patterns.
- Core changes still require an explicitly requested extraction meeting every
  condition in AGENTS.md or separate explicit authorisation for broader work.
  Document extracted operations, destinations, API/security preservation,
  focused tests, and applicable Windows checks when relevant.

Report vulnerabilities through [SECURITY.md](SECURITY.md), never as a public issue or in a PR description.
