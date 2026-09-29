## What changed and why

<!-- Explain the concrete change and reason. Link related issues. -->

## User-visible behavior

<!-- Describe what users notice, or state that behavior is unchanged. -->

## Gotchas

<!-- Include edge cases, environment assumptions, and outstanding verification. -->

## How to test

<!-- Give concrete commands and applicable Windows/Roblox manual steps. -->

- [ ] Full pre-commit sequence passed: format, check, workspace tests, strict Clippy, frontend lint, and type checking.
- [ ] Changelog updated under `## Unreleased` for user-facing changes, or N/A.
- [ ] Design and security guidance checked where touched, or N/A.
- [ ] No secrets, local data, generated output, or scratch files in the diff.
- [ ] Diff reviewed end-to-end; staged changes match the task.
- [ ] Persisted state uses crash-safe core storage helpers; IPC inputs are validated.
- [ ] Core edits meet the limited extraction policy, or separate explicit authorisation is recorded; operations, destinations, tests, and Windows checks are documented where applicable.

### Interface changes

- [ ] Reused existing controls in `ram_ui/frontend/components/` and shared values in `ram_ui/frontend/tokens.css`.
- [ ] Checked against `DESIGN.md`, including Roboto, icons, colors, shape, and motion.
- [ ] Verified applicable keyboard/focus, loading, error/retry, disabled, and saving states.
- [ ] Checked supported window size, display scaling, menu clipping, and reduced motion.
- [ ] Documented any new reusable component in `DESIGN.md`; no component exists only to wrap a single element.

### Screenshots or recordings

<!-- Add interface visuals when useful, or write Not applicable. -->
