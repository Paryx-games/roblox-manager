## Summary

<!-- What changed, and why? Link related issues with `Closes #123` when applicable. -->

## Changes

-

## Testing

<!-- List the commands and manual checks you ran. -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo test --workspace`
- [ ] `pnpm --dir ram_ui lint` (includes stylelint design-token checks)
- [ ] `pnpm --dir ram_ui typecheck`
- [ ] Manual testing completed on Windows with Roblox installed, where applicable

## Review checklist

- [ ] The change is focused and preserves unrelated existing behaviour.
- [ ] User-facing changes have a `CHANGELOG.md` entry under `## Unreleased`.
- [ ] Persisted application state uses crash-safe storage helpers (`ram_core::storage::atomic_write`/`atomic_swap`), never a bare `std::fs::write`.
- [ ] Logs, error messages, Tauri command error payloads, screenshots, and other output do not expose cookies, tokens, passwords, or other secrets.
- [ ] Data crossing the Tauri IPC boundary (frontend → command, and command → frontend) is treated as untrusted / validated, not assumed safe because it's "our own UI."
- [ ] The PR title and commits follow Conventional Commits.
- [ ] I have not included secrets, generated files, `node_modules/`, or unrelated changes.

## Design system checklist (UI changes only - see `DESIGN.md`)

- [ ] Any new UI uses existing components from `ram_ui/src/components/` and tokens from `ram_ui/src/tokens.css` - no raw hex, `box-shadow`, or non-standard `border-radius` (these fail CI via stylelint, but were checked before pushing).
- [ ] No off-scale spacing or font-size value without a genuine structural reason (e.g. a fixed layout dimension) - see `DESIGN.md` § Enforcement for the hard-rule vs. judgment-call split.
- [ ] Every new interactive component implements its full required interaction-state set from `DESIGN.md` § Interaction states (hover/focus/active/disabled/loading, as applicable) - a missing focus ring is not a follow-up.
- [ ] No new component was created solely to wrap a single element (`DESIGN.md` § 4).
- [ ] If I added a new reusable component, it's documented in `DESIGN.md` § 4 and added to `/dev/design-system`.
- [ ] No emoji used as icons; icons come from `assets/icons` only.

## Screenshots or recordings

<!-- Add visuals for UI changes, or write "Not applicable". -->
