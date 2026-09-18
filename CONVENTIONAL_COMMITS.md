# Conventional Commits format

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification. Your commit messages should be structured as follows.

<type>(optional scope): <description>

> [!NOTE]
> The `scope` is optional but recommended - see the rules below before picking one.

| Type         | When to use                                                            |
| ------------ | ---------------------------------------------------------------------- |
| **feat**     | A new feature                                                          |
| **fix**      | A bug fix                                                              |
| **docs**     | Documentation only changes                                             |
| **style**    | Formatting, missing semi-colons, whitespace, etc. (no code changes)    |
| **refactor** | Code changes that neither fix a bug nor add a feature                  |
| **perf**     | Code changes that improve performance                                  |
| **test**     | Adding or fixing tests                                                 |
| **build**    | Changes to build system or external dependencies (npm, electron, etc.) |
| **ci**       | Changes to CI config or scripts                                        |
| **chore**    | Other changes that don't modify src or test files (cleanup, tooling)   |
| **revert**   | Reverting a previous commit                                            |

## Scopes

A scope names the **feature domain or module actually touched** - not the layer (`ui`, `backend`), not the file type, and not something generic enough to fit half the repo. If you can't name a specific thing from the list below that the change belongs to, it's a signal the commit might be doing too much, or that no scope is genuinely more honest than a vague one.

**Rule of thumb:** would someone scanning `git log --oneline` know roughly what changed just from the scope, without opening the diff? `style(ui)` fails that test on every UI commit ever made. `style(tokens)` or `style(accounts)` passes.

### Feature-domain scopes (frontend page + its backend command module, treated as one unit)

RM's pages and their Tauri command handlers are 1:1 (see AGENTS.md Architecture), so use the domain name regardless of whether the change landed in the React page, the command handler, or both:

| Scope         | Covers                                                                  |
| ------------- | ----------------------------------------------------------------------- |
| **dashboard** | Dashboard page + related commands                                       |
| **accounts**  | Accounts page, account CRUD, import/export, `commands/accounts`         |
| **instances** | Instance list, launch/kill flows, window tiling, `commands/instances`   |
| **groups**    | Group browsing/membership, `commands/groups`, `group_api.rs`            |
| **assets**    | Asset Manager, uploads, `commands/assets`, `assets.rs`, `assets_api.rs` |
| **activity**  | Activity/presence page, `commands/presence`                             |
| **settings**  | Settings page, app config, startup toggle                               |

### `ram_core` module scopes (no frontend counterpart)

| Scope       | Covers                                               |
| ----------- | ---------------------------------------------------- |
| **crypto**  | `crypto.rs` - envelope encryption, Argon2id, AES-GCM |
| **storage** | `storage.rs` - atomic writes, `.bak` handling        |
| **auth**    | `auth.rs` - `RobloxClient`, CSRF caching, backoff    |
| **api**     | `api.rs` - general Roblox REST endpoints             |
| **process** | `process.rs` - Win32 process/mutex/window logic      |
| **redact**  | `redact.rs` - log scrubbing rules                    |
| **presets** | `presets.rs` - per-file preset persistence           |

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

### Omit the scope entirely when

- The change genuinely spans multiple domains and picking one would be misleading (e.g. a workspace-wide lint config change).
- It's a top-level doc file not covered above (`README.md`, `AGENTS.md`, `SECURITY.md`) - just use `docs:` with no scope, or `docs(agents)` / `docs(security)` if you want to be specific about which doc.

### Explicitly avoid these as scopes

- **`ui`** - matches almost every frontend commit; use the page/domain scope instead (`accounts`, `tokens`, `layout`, etc.)
- **`misc`** / **`general`** - if nothing specific fits, that's a sign to either omit the scope or split the commit
- **A version number as scope** (e.g. `docs(v1.16.0)`) - versions aren't domains; use `changelog` or `release` instead
- **Singular/plural drift** - it's `accounts`, not sometimes `account` and sometimes `accounts`. Match the folder/module name exactly.
