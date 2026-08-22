# Conventional Commits format

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification. Your commit messages should be structured as follows.

```
<type>(optional scope): <description>
```

> [!NOTE]
> The `scope` is optional but recommended.

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
| **chore**    | Other changes that don’t modify src or test files (cleanup, tooling)   |
| **revert**   | Reverting a previous commit                                            |
