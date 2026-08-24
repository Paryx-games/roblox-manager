<p align="center">
  <img src="assets/branding/LogoThumb.png" alt="roblox manager" width="650">
</p>

<p align="center">
  <a href="https://github.com/Paryx-games/roblox-manager/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/Paryx-games/roblox-manager/ci.yml?label=ci&logo=github&logoColor=white&color=9333ea" alt="ci">
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

## Features

- **Multi-Account Management** - Add, remove, and organize Roblox accounts with cookie-based auth
- **Encrypted Storage** - AES-256-GCM, unlocked automatically via Windows Credential Manager. An optional master password (Argon2id) is available for anyone who wants one
- **Multi-Instance** - Launch multiple Roblox clients simultaneously
- **Bulk Launch** - Launch selected accounts into the same server sequentially
- **Privacy Mode** - Clears tracking cookies before each launch
- **Auto Window Tiling** - Arranges Roblox windows in a grid after launch
- **Live Presence** - Real-time Online / In Game / In Studio / Offline indicators

> [!IMPORTANT]
> RM stores Roblox authentication cookies in encrypted form. Never share your `.ROBLOSECURITY` cookie with anyone, and treat it like a password.
>
> If you are planning to contribute, please read the [commit guide](CONVENTIONAL_COMMITS.md) and [contributing guide](CONTRIBUTING.md) before opening a pull request. Changes involving cookie handling, encryption, storage, or process control require additional review and should be explicitly mentioned in the PR description.
>
> Never include `.ROBLOSECURITY` cookies, authentication tokens, credentials, encryption keys, or other sensitive account data in issues, pull requests, commits, logs, or screenshots. If you discover a security vulnerability, see [SECURITY.md](SECURITY.md) for how to report it privately.

## Building from Source

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- Windows 10/11 (required for Win32 APIs)

### Build

```bash
# Clone the repository
git clone https://github.com/Paryx-games/roblox-manager.git
cd roblox-manager

# Build in release mode
cargo build --release

# Run
cargo run --release
```

The compiled binary will be at `target/release/ram_ui.exe`.

### Development

```powershell
# Check for errors without building
cargo check

# Run with debug logging
$env:RUST_LOG="debug"; cargo run
```

> [!TIP]
> If you are developing RM, `cargo check` is the quickest way to catch compilation errors without producing a release build.

## Usage

1. **First launch** - Nothing to set up. Encryption configures itself on this PC
2. **Add accounts** - Click "+ Add Account" and paste your `.ROBLOSECURITY` cookie
3. **Launch** - Select an account, enter a Place ID, and click Launch
4. **Bulk launch** - Ctrl+click or Shift+click to select multiple accounts, then use the group panel
5. **Settings** - Configure multi-instance, privacy mode, auto-arrange, and more

> [!CAUTION]
> Multi-instance and privacy features interact with Roblox's local processes and files. Roblox updates may change or break these behaviours, so do not assume that a feature will continue working indefinitely.

## Credits

- [RobloxManager](https://gitlab.com/centerepic/robloxmanager) by [centerepic](https://gitlab.com/centerepic) - The modern version of RobloxAccountManager that this repository was forked from
- [RobloxAccountManager](https://github.com/ic3w0lf22/Roblox-Account-Manager) by [ic3w0lf22](https://github.com/ic3w0lf22) - The original Roblox Account Manager that served as the primary reference for this project

## Background

RM is the spiritual successor to [ByeBanAsync](https://github.com/centerepic/ByeBanAsync), since simply clearing `RobloxCookies.dat` is no longer effective on its own. The project focuses on managing separate Roblox sessions and account data while adapting to changes in Roblox's client behaviour.

Later updates may be made to reinforce account isolation and session management if needed.

## License

[MIT](LICENSE)
