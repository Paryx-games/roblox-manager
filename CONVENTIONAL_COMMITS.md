# Conventional Commits format

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification, with a few project-specific tightenings below. Commit messages are structured as:

```
<type>(<scope>)[!]: <description>

[optional body]

[optional footer, e.g. BREAKING CHANGE: ...]
```

> [!IMPORTANT]
> **For AI agents and contributors - read this before writing any commit message.**
>
> 1. **Pick the scope from the tables below, exactly as spelled.** Do not invent scopes.
> 2. **Never use `ui` as a scope.** Not `feat(ui)`, not `fix(ui)`, not `refactor(ui)`. Ever. Use the page or domain the change is about (see [Choosing a scope](#choosing-a-scope)).
> 3. **`style` is formatting only.** Whitespace, semicolons, import order, formatter runs. It is **not** for UI, CSS, or visual changes - see [Types](#types).
> 4. **Mark breaking changes.** Add `!` after the scope and a `BREAKING CHANGE:` footer. See [Breaking changes](#breaking-changes).
> 5. **One commit, one scope.** No comma-separated scopes. If a change needs two, split it or omit the scope.

## Message rules

- Subject line is **lowercase**, **imperative mood** ("add", not "added" or "adds"), **no trailing period**, and ideally **72 characters or fewer**.
- Describe _what changed for the app or user_, not which files you touched. `fix(auth): retry csrf fetch after 403` beats `fix(auth): update auth.rs`.
- Body is optional. Use it for the _why_ when the subject alone won't explain it.
- Don't put the type twice (`feat(accounts): feat: add import`).
- One logical change per commit. If the subject needs "and", it's probably two commits.

## Types

| Type         | When to use                                                                                                                        |
| ------------ | ---------------------------------------------------------------------------------------------------------------------------------- |
| **feat**     | A new feature or user-visible capability, including visual changes that improve or add to how the app looks                        |
| **fix**      | A bug fix, including visual bugs (misaligned, clipped, wrong color, broken layout)                                                 |
| **docs**     | Documentation only changes                                                                                                         |
| **style**    | Formatting only: whitespace, semi-colons, import order, formatter/linter autofix. **Must not change how the app looks or behaves** |
| **refactor** | Code changes that neither fix a bug nor add a feature                                                                              |
| **perf**     | Code changes that improve performance                                                                                              |
| **test**     | Adding or fixing tests                                                                                                             |
| **build**    | Changes to build system or external dependencies (cargo, pnpm, tauri config, etc.)                                                 |
| **ci**       | Changes to CI config or scripts                                                                                                    |
| **chore**    | Other changes that don't modify src or test files (cleanup, tooling)                                                               |
| **revert**   | Reverting a previous commit                                                                                                        |

### Picking between similar types

- **`style` is not for UI changes.** If a user could see the difference (spacing, colors, layout, new look), it's `feat` (new or improved) or `fix` (something was broken or wrong), never `style`. If nobody could tell the difference except by reading the diff, it's `style` or `refactor`.
- **`style` vs `refactor`:** `style` changes how code is _written_ (formatting) without touching structure. `refactor` changes how code is _structured_.
- **`fix` vs `test`:** if only test files changed, it's `test`, even if the commit "fixes" a flaky test.
- **`build` vs `ci` vs `chore`:** `build` is how the app is compiled, bundled, or which dependencies it uses. `ci` is the pipeline config. `chore` is everything else that isn't source or tests.
- **`chore` never touches src.** If it changes app behavior, it's a `feat`, `fix`, `refactor`, or `perf`.

## Scopes

A scope names the **feature domain or module actually touched** - not the layer (`ui`, `backend`), not the file type, and not something generic enough to fit half the repo. If you can't name a specific thing from the lists below that the change belongs to, it's a signal the commit might be doing too much, or that no scope is more honest than a vague one.

**Rule of thumb:** would someone scanning `git log --oneline` know roughly what changed just from the scope, without opening the diff? `fix(ui)` fails that test on every UI commit ever made. `feat(tokens)` or `fix(accounts)` passes.

### Feature-domain scopes (frontend page + its backend command module, treated as one unit)

RM's pages and their Tauri command handlers are 1:1 (see AGENTS.md Architecture), so use the domain name regardless of whether the change landed in the React page, the command handler, or both:

| Scope         | Covers                                                                  |
| ------------- | ----------------------------------------------------------------------- |
| **dashboard** | Main dashboard and overview panels                                      |
| **accounts**  | Account management, account CRUD, and import/export                     |
| **instances** | Roblox launching, instance tracking, process association, and tiling   |
| **groups**    | Group browsing, membership, and group panels                            |
| **assets**    | Asset Manager, asset validation, and uploads                            |
| **activity**  | Presence and activity behavior                                          |
| **settings**  | Settings, app configuration, and startup behavior                       |
| **presets**   | Saved game and launch presets                                           |

### `ram_core` module scopes (no frontend counterpart)

| Scope       | Covers                                               |
| ----------- | ---------------------------------------------------- |
| **crypto**  | `crypto.rs` - envelope encryption, Argon2id, AES-GCM |
| **storage** | `storage.rs` - atomic writes, `.bak` handling        |
| **auth**    | `auth.rs` - `RobloxClient`, CSRF caching, backoff    |
| **api**     | `api.rs` - general Roblox REST endpoints             |
| **process** | `process.rs` - Win32 process/mutex/window logic      |
| **redact**  | `redact.rs` - log scrubbing rules                    |
| **models**  | `models.rs` - shared account and app models           |
| **error**   | `error.rs` - shared error types                       |
| **multipart** | `multipart.rs` - multipart request encoding          |

### Frontend infrastructure scopes (not tied to one page)

| Scope          | Covers                                                             |
| -------------- | ------------------------------------------------------------------ |
| **tokens**     | `tokens.css` - design token additions/changes                      |
| **components** | Shared components in `components/` (Card, DataTable, Button, etc.) |
| **layout**     | App shell - activity bar, sidebar, routing (`App.tsx`)             |
| **ipc**        | `lib/ipc.ts` - typed Tauri invoke wrappers                         |

### Project-wide scopes

| Scope         | Covers                                                                    |
| ------------- | ------------------------------------------------------------------------- |
| **release**   | Version bumps, tag prep                                                   |
| **changelog** | Changes to `CHANGELOG.md` itself (formatting, backfilling)                |
| **deps**      | Dependency bumps with no behavior change (`Cargo.lock`, `pnpm-lock.yaml`) |

## Choosing a scope

Work through these in order and stop at the first one that fits.

1. **Ask what the commit is _about_, not what files it touches.** Adding a filter to the accounts page that also needs a small tweak in `components/` is `accounts`, because the purpose is the accounts feature. A commit whose whole point is changing the shared `DataTable` is `components`.
2. **Match the changed files to a scope** using the tables above. The `Covers` column lists the files and folders each scope owns.
3. **Multiple files, same domain** (page + its command handler + its API module) - use the domain scope.
4. **Multiple unrelated domains** - split it into separate commits. If it truly can't be split (workspace-wide lint config, repo-wide rename), omit the scope.
5. **Nothing fits** - omit the scope. A missing scope is honest; a made-up one is noise.

### Omit the scope entirely when

- The change genuinely spans multiple domains and picking one would be misleading (e.g. a workspace-wide lint config change).
- It's a top-level doc file not covered above (`README.md`, `AGENTS.md`, `SECURITY.md`) - just use `docs:` with no scope, or `docs(agents)` / `docs(security)` if you want to be specific about which doc.

### Never use these as scopes

| Don't use                                                      | Why                                                                                        | Use instead                                            |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------ |
| **`ui`**, **`frontend`**, **`backend`**, **`core`**, **`app`** | These are layers, not domains. They match almost every commit                              | The page/domain scope (`accounts`, `tokens`, `layout`) |
| **`css`**, **`react`**, **`rust`**, **`tauri`**, **`tsx`**     | File types or tech names, not features                                                     | The domain the file belongs to                         |
| **`misc`**, **`general`**, **`various`**, **`other`**          | If nothing specific fits, either omit the scope or split the commit                        | Omit the scope, or split                               |
| **`v1.16.0`** (any version number)                             | Versions aren't domains                                                                    | `changelog` or `release`                               |
| **`account`** (singular), **`Accounts`** (capitalized)         | Scopes must match the folder/module name exactly - lowercase, and no singular/plural drift | `accounts`                                             |
| **`accounts,groups`** (multiple)                               | One scope per commit                                                                       | Split the commit, or omit the scope                    |
| **`auth.rs`** (a filename)                                     | Scopes name a module, not a file                                                           | `auth`                                                 |

## Breaking changes

Breaking changes are allowed. When you make one, mark it in **both** places:

1. Put `!` right after the scope (or right after the type if there's no scope), before the colon.
2. Add a `BREAKING CHANGE:` footer (uppercase, exactly that spelling) after a blank line, saying what breaks and what a user needs to do about it.

The `!` works on any type (`feat!`, `fix!`, `refactor!`, ...). It doesn't change the one-scope rule.

```
feat(storage)!: switch account store to versioned format

existing stores are migrated on first launch, but older app
versions can no longer open the new format.

BREAKING CHANGE: account store written by this version cannot be
read by earlier versions. downgrading requires restoring from backup.
```

### What counts as breaking here

A change is breaking if, after updating, a user's **existing install, saved data, or exported files** stop working or need manual action.

- Storage or encryption format changes that older versions can't read, or that don't migrate automatically
- Changed import/export file formats
- Renamed or removed settings or config keys without a migration
- Removing a feature people rely on

**Not** breaking: internal refactors, new optional features, and IPC signature changes between `ram_ui` and `ram_core` (they ship together).

## Examples by type

Each block shows what to do and what not to do. Comments after `#` explain the mistake and are not part of the message.

### feat

```
# good
feat(accounts): add bulk cookie import
feat(instances): add tile-to-grid window layout
feat(tokens): add compact spacing scale for dense tables

# bad
feat(ui): add bulk cookie import          # layer scope - should be accounts
feat(accounts): Added bulk import.        # capitalized, past tense, trailing period
feat(accounts): add import and fix export # two changes - split into feat and fix
```

### fix

```
# good
fix(auth): retry csrf fetch after 403
fix(groups): stop crash on empty group list
fix(components): stop datatable header clipping on narrow windows

# bad
fix(frontend): crash on empty group list  # layer scope - should be groups
fix(auth.rs): handle 429s                 # filename as scope - should be auth
fix: fixed the thing                      # vague and past tense
```

### docs

```
# good
docs(agents): clarify roblox api error handling
docs(changelog): backfill missing 1.15 entries
docs: add build instructions to readme

# bad
docs(readme): fix typo                    # readme isn't a scope - use docs: with no scope
docs(v1.16.0): update changelog           # version as scope - should be docs(changelog)
docs: updated docs                        # vague, past tense
```

### style

Formatting only. If the app looks or behaves any differently afterwards, it is not `style`.

```
# good
style(storage): run rustfmt on storage module
style(components): fix inconsistent indentation in datatable
style: run cargo fmt and prettier across the workspace

# bad
style(ui): update button padding          # layer scope, and padding is a visual change - use feat/fix(components)
style(tokens): tighten spacing scale      # changes how the app looks - use feat(tokens) or fix(tokens)
style(accounts): make table rows compact  # visual change - use feat(accounts) or fix(accounts)
```

### refactor

```
# good
refactor(storage): extract atomic write helper
refactor(components): split datatable into header and body
refactor(process): replace raw handles with a wrapper type

# bad
refactor(auth): fix csrf race and tidy up # fixes a bug - that's fix (and tidying is a separate commit)
refactor: cleanup                         # vague
refactor(core): move stuff around         # layer scope and vague
```

### perf

```
# good
perf(instances): batch window tiling calls
perf(process): cache window handles between polls
perf(accounts): virtualize the account table

# bad
perf(instances): make launching better    # vague, says nothing about what got faster
perf(ui): memoize rows                    # layer scope - should be accounts or components
```

### test

```
# good
test(crypto): add roundtrip tests for envelope encryption
test(storage): cover .bak recovery after partial write

# bad
test(tests): add tests                    # scope names the file type - use the module under test
fix(crypto): fix flaky roundtrip test     # only tests changed - should be test(crypto)
```

### build

```
# good
build(deps): bump tauri to 2.x
build: enable lto in release profile
build: pin pnpm version in package.json

# bad
build(pnpm-lock.yaml): update deps        # filename as scope - should be deps
build: run clippy on pull requests        # pipeline config - should be ci
```

### ci

```
# good
ci: cache cargo registry in build workflow
ci: run clippy on pull requests

# bad
ci(github): add workflow                  # github isn't a scope - omit it
ci: fixed pipeline                        # vague and past tense
```

### chore

```
# good
chore: remove unused scripts
chore: update gitignore

# bad
chore(auth): fix csrf caching             # touches src and changes behavior - should be fix(auth)
chore: misc cleanup                       # misc and vague - say what was cleaned up
```

### revert

Use the `revert` type, name what was reverted, and put the hash in the body.

```
# good
revert(accounts): revert "feat(accounts): add bulk cookie import"

This reverts commit 1a2b3c4.

# bad
fix(accounts): remove bulk import         # undoing a commit is a revert
revert: undo last commit                  # doesn't say what was reverted
```

### breaking changes

```
# good
feat(storage)!: switch account store to versioned format
refactor(settings)!: rename config keys and migrate old values
fix(crypto)!: bump argon2id parameters

  (each with a BREAKING CHANGE: footer explaining what breaks)

# bad
feat!(storage): switch account store format   # the ! goes after the scope, not before it
feat(storage): switch account store format    # breaks existing data but isn't marked
feat(storage)!: add optional retry setting    # not breaking - no !
feat(storage)!: switch store format           # marked, but no BREAKING CHANGE: footer
breaking change: old stores unreadable        # footer must be uppercase BREAKING CHANGE:
```

## Quick self-check before committing

- Is the scope copied exactly from a table above (or omitted)?
- Is it one scope, lowercase, matching the folder/module name?
- Would `git log --oneline` tell a stranger roughly what changed?
- Is the subject lowercase, imperative, and free of a trailing period?
- If it's `style`, would the app look and behave exactly the same? If not, use `feat` or `fix`.
- If it breaks existing installs or data, does it have both `!` and a `BREAKING CHANGE:` footer?
