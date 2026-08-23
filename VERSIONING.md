# Versioning

RM follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- **MAJOR** — breaking changes to config/account-store format that require migration, or removal of a documented feature.
- **MINOR** — new features, added settings, new tabs/capabilities. Backward-compatible.
- **PATCH** — bug fixes, wording/UI polish, no new capability.

## Pre-releases

RM has no 0.x or beta phase. Every release is a stable `MAJOR.MINOR.PATCH` version.

Pre-release identifiers (`-beta.N`, `-rc.N`) are not currently used. If they're introduced in the future:

- **`-beta.N`** (e.g. `v2.0.0-beta.1`) — an early build of an upcoming version, published for testing before it's considered feature-complete or stable. Expect bugs.
- **`-rc.N`** ("release candidate", e.g. `v2.0.0-rc.1`) — a build believed ready to ship, published for a final round of testing before the real tag goes out. If no issues turn up, the next tag is the plain version with no suffix.

Under SemVer, a pre-release version sorts *before* the plain version it leads up to (`v2.0.0-rc.1` < `v2.0.0`), and tooling generally should not treat a pre-release as the "latest" stable version.

## Where the version lives

The version lives once in the root `Cargo.toml` and both crates inherit it — do not hardcode a version anywhere else.
