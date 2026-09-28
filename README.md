# ⚠️ LEGACY (PRE-V2) - DO NOT USE THIS VERSION OF THE APP

> [!CAUTION]
> **This is the version of RM from before the v2 rework. It is frozen permanently and will not be updated any more after this commit.**
> No features, no bug fixes, no security patches, no support.
>
> **If you would like to use the stable version, go to the [`main` branch](https://github.com/Paryx-games/roblox-manager).**

<p align="center">
  <img src="assets/branding/LogoThumb.png" alt="roblox manager" width="650">
</p>

<p align="center">
  <a href="https://github.com/Paryx-games/roblox-manager/actions/workflows/rust.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/Paryx-games/roblox-manager/rust.yml?label=ci&logo=github&logoColor=white&color=9333ea" alt="ci">
  </a>
  <a href="https://github.com/Paryx-games/roblox-manager/actions/workflows/release.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/Paryx-games/roblox-manager/release.yml?label=release&logo=github&logoColor=white&color=7c3aed" alt="release">
  </a>
  <a href="https://github.com/Paryx-games/roblox-manager/releases/latest">
    <img src="https://img.shields.io/github/v/release/Paryx-games/roblox-manager?label=version&logo=github&logoColor=white&color=6366f1" alt="version">
  </a>
  <img src="https://img.shields.io/github/downloads/Paryx-games/roblox-manager/total?label=downloads&logo=github&logoColor=white&color=4f46e5" alt="downloads">
  <img src="https://img.shields.io/github/license/Paryx-games/roblox-manager?label=license&logo=github&logoColor=white&color=3b82f6" alt="license">
  <img src="https://img.shields.io/github/stars/Paryx-games/roblox-manager?style=flat&label=stars&logo=github&logoColor=white&color=0ea5e9" alt="stars">
  <img src="https://img.shields.io/github/issues/Paryx-games/roblox-manager?label=issues&logo=github&logoColor=white&color=14b8a6" alt="issues">
</p>

> [!NOTE]
> This repository is a fork of [gitlab.com/centerepic/robloxmanager](https://gitlab.com/centerepic/robloxmanager).

A fast, lightweight Roblox account manager built with Rust and [egui](https://github.com/emilk/egui). Manage multiple Roblox accounts, launch games, and switch between sessions with ease.

**[Visit the RM website](https://paryx-games.github.io/roblox-manager/)** for more information, including additional details about features and the project.

> [!WARNING]
> This tool interacts with Roblox authentication cookies and game-launching internals. Use it at your own risk. The multi-instance feature bypasses Roblox's singleton mutex, which may conflict with Hyperion anti-cheat and could carry a ban risk. This project is not affiliated with or endorsed by Roblox Corporation.

> [!NOTE]
> This project is independent and is not affiliated with, endorsed by, or sponsored by Roblox Corporation.

## Status

| | |
|---|---|
| **Version** | Pre-v2 (last egui-based UI) |
| **State** | Frozen permanently |
| **Maintained** | No |
| **Accepting issues** | No |
| **Accepting pull requests** | No |
| **Security fixes** | No |
| **Stable version** | [`main` branch](https://github.com/Paryx-games/roblox-manager) |

> [!WARNING]
> Everything below describes the app **as it was before the v2 rework**. It is kept for reference and archival purposes only, and none of it is guaranteed to still work.

## Before the v2 Rework

This snapshot is RM as it existed before the v2 rework. At this point the UI was built with egui/eframe and everything (core logic and UI) lived in the same Rust-only app.

The v2 rework replaced the egui/eframe UI with a Tauri + React/TypeScript frontend. It only touched the presentation layer: `ram_core` stayed responsible for the core logic (authentication, storage, cryptography, Roblox API handling, and Win32 process management). v2 also came with a new design system, a typed IPC layer between the frontend and Rust, and new development tooling for the frontend stack.

The v2 work is tracked in [#29](https://github.com/Paryx-games/roblox-manager/issues/29) and [#30](https://github.com/Paryx-games/roblox-manager/pull/30).

> [!NOTE]
> Nothing in this branch reflects the v2 UI, the v2 design system, or the v2 tooling. If a feature, screenshot, or workflow you are looking for does not appear here, that is why.

## Why It Is Frozen

Development moved on from this version. It is locked so nobody runs an outdated build that handles authentication cookies, and so the pre-v2 history stays intact for anyone who wants to look back at it.

Roblox client behaviour changes constantly. Multi-instance, roblox apis, login and a lot of other things all depend on local process and file internals, so this build **will** drift out of sync with Roblox over time and nothing will be fixed when it does.

## Moving to the Stable Version

1. Go to the [`main` branch](https://github.com/Paryx-games/roblox-manager) or the [latest release](https://github.com/Paryx-games/roblox-manager/releases/latest).
2. Download or build the current version there.
3. Stop using this legacy build.

## Features (Pre-v2)

- **Multi-Account Management** - Add, remove, and organize Roblox accounts with cookie-based auth
- **Encrypted Storage** - AES-256-GCM, unlocked automatically via Windows Credential Manager. An optional master password (Argon2id) is available for anyone who wants one
- **Multi-Instance** - Launch multiple Roblox clients simultaneously
- **Bulk Launch** - Launch selected accounts into the same server sequentially
- **Privacy Mode** - Clears tracking cookies before each launch
- **Discord Webhooks** - Sends optional notifications through a protected Discord webhook
- **Auto Window Tiling** - Arranges Roblox windows in a grid after launch
- **Live Presence** - Real-time Online / In Game / In Studio / Offline indicators

> [!IMPORTANT]
> This version will **never** receive security patches. If a vulnerability is found in it, it stays unpatched. That alone is a good reason to move to `main`.
>
> RM stores Roblox authentication cookies in encrypted form. Never share your `.ROBLOSECURITY` cookie with anyone, and treat it like a password.
>
> Discord webhook URLs are sensitive bearer credentials. RM keeps them out of `config.json` and stores them in Windows Credential Manager, alongside the device key. Never share a webhook URL publicly; rotate it in Discord if it is exposed.
>
> This branch does not accept contributions. If you are planning to contribute, do it against the [`main` branch](https://github.com/Paryx-games/roblox-manager) and read the [commit guide](CONVENTIONAL_COMMITS.md) and [contributing guide](CONTRIBUTING.md) before opening a pull request. Changes involving cookie handling, encryption, storage, or process control require additional review and should be explicitly mentioned in the PR description.
>
> Never include `.ROBLOSECURITY` cookies, authentication tokens, credentials, encryption keys, or other sensitive account data in issues, pull requests, commits, logs, or screenshots. If you discover a security vulnerability, see [SECURITY.md](SECURITY.md) for how to report it privately.

## Building from Source (Archival Use Only)

> [!NOTE]
> This is only here for people who want to inspect or study the pre-v2 code. For actual use, build from `main`.

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- Windows 10/11 (required for Win32 APIs)

### Build

```bash
# clone the repository
git clone https://github.com/Paryx-games/roblox-manager.git
cd roblox-manager

# build in release mode
cargo build --release

# run
cargo run --release
```

The compiled binary will be at `target/release/ram_ui.exe`.

### Development

```powershell
# check for errors without building
cargo check

# run with debug logging
$env:RUST_LOG="debug"; cargo run
```

> [!TIP]
> `cargo check` is the quickest way to catch compilation errors without producing a release build. This only applies to the pre-v2 egui build, since v2 has a separate frontend toolchain.

## Usage (Pre-v2)

1. **First launch** - Nothing to set up. Encryption configures itself on this PC
2. **Add accounts** - Click "+ Add Account" and paste your `.ROBLOSECURITY` cookie
3. **Launch** - Select an account, enter a Place ID, and click Launch
4. **Bulk launch** - Ctrl+click or Shift+click to select multiple accounts, then use the group panel
5. **Settings** - Configure multi-instance, privacy mode, auto-arrange, and more

> [!CAUTION]
> Multi-instance and privacy features interact with Roblox's local processes and files. Roblox updates may change or break these behaviours, and since this version is frozen, nothing will be fixed when that happens.

## Credits

- [RobloxManager](https://gitlab.com/centerepic/robloxmanager) by [centerepic](https://gitlab.com/centerepic) - The modern version of RobloxAccountManager that this repository was forked from
- [RobloxAccountManager](https://github.com/ic3w0lf22/Roblox-Account-Manager) by [ic3w0lf22](https://github.com/ic3w0lf22) - The original Roblox Account Manager that served as the primary reference for this project
- [Lucide Icons](https://lucide.dev/icons/) - The icon pack used for icons throughout the app. Licensed under the [ISC License](https://github.com/lucide-icons/lucide/blob/main/LICENSE).

## Background

RM is the spiritual successor to [ByeBanAsync](https://github.com/centerepic/ByeBanAsync), since simply clearing `RobloxCookies.dat` is no longer effective on its own. The project focuses on managing separate Roblox sessions and account data while adapting to changes in Roblox's client behaviour.

## License

[MIT](LICENSE)
