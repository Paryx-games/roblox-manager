<a id="readme-top"></a>

<div align="center">
  <a href="https://github.com/Paryx-games/roblox-manager">
    <img src="assets/branding/LogoThumb.png" alt="Logo" width="400">
  </a>

  <h3 align="center">Roblox Manager</h3>

  <p align="center">
    A lightweight multi-account manager using Tauri and Rust, featuring low RAM & CPU usage, easy-to-navigate UI and tons of features.
    <br />
    <a href="https://roblox-manager.gitbook.io/docs"><strong>Explore the docs »</strong></a>
    <br />
    <br />
    <a href="https://paryx-games.github.io/roblox-manager/">Our Website</a>
    &middot;
    <a href="https://github.com/Paryx-games/roblox-manager/issues/new?template=bug_report.yml">Report Bug</a>
    &middot;
    <a href="https://github.com/Paryx-games/roblox-manager/issues/new?template=feature_request.yml">Request Feature</a>
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
    <img src="https://img.shields.io/github/forks/Paryx-games/roblox-manager?style=flat&label=forks&logo=github&logoColor=white&color=14b8a6" alt="forks">
    <img src="https://img.shields.io/github/issues/Paryx-games/roblox-manager?label=issues&logo=github&logoColor=white&color=10b981" alt="issues">
  </p>
</div>

> [!NOTE]
> This repository is a fork of [gitlab.com/centerepic/robloxmanager](https://gitlab.com/centerepic/robloxmanager).

<details>
  <summary>Table of Contents</summary>
  <ol>
    <li><a href="#about-the-project">About The Project</a></li>
    <li><a href="#features">Features</a></li>
    <li><a href="#usage">Usage</a></li>
    <li><a href="#credits">Credits</a></li>
    <li>
      <a href="#building-from-source">Building From Source</a>
      <ul>
        <li><a href="#prerequisites">Prerequisites</a></li>
        <li><a href="#build">Build</a></li>
      </ul>
    </li>
    <li><a href="#development-commands">Development Commands</a></li>
    <li><a href="#reporting-issues--requesting-features">Reporting Issues & Requesting Features</a></li>
    <li><a href="#contributing">Contributing</a></li>
    <li><a href="#license">License</a></li>
  </ol>
</details>

## About The Project

A fast, lightweight Roblox account manager built with Rust and [egui](https://github.com/emilk/egui). Manage multiple Roblox accounts, launch games, and switch between sessions with ease.

**[Visit the RM website](https://paryx-games.github.io/roblox-manager/)** for more information, including additional details about features and the project.

> [!WARNING]
> This tool interacts with Roblox authentication cookies and game-launching internals. Use it at your own risk. The multi-instance feature bypasses Roblox's singleton mutex, which may conflict with Hyperion anti-cheat and could carry a ban risk. This project is not affiliated with or endorsed by Roblox Corporation.

> [!NOTE]
> This project is independent and is not affiliated with, endorsed by, or sponsored by Roblox Corporation.

### Built With

- [Rust](https://www.rust-lang.org/)
- [egui](https://github.com/emilk/egui)
- [Tauri](https://tauri.app/)
- [React](https://react.dev/)

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Features

- **Multi-Account Management** - Add, remove, and organize Roblox accounts with cookie-based auth
- **Encrypted Storage** - AES-256-GCM, unlocked automatically via Windows Credential Manager. An optional master password (Argon2id) is available for anyone who wants one
- **Multi-Instance** - Launch multiple Roblox clients simultaneously
- **Bulk Launch** - Launch selected accounts into the same server sequentially
- **Privacy Mode** - Clears tracking cookies before each launch
- **Discord Webhooks** - Sends optional notifications through a protected Discord webhook
- **Auto Window Tiling** - Arranges Roblox windows in a grid after launch
- **Live Presence** - Real-time Online / In Game / In Studio / Offline indicators

> [!IMPORTANT]
> RM stores Roblox authentication cookies in encrypted form. Never share your `.ROBLOSECURITY` cookie with anyone, and treat it like a password.
>
> Discord webhook URLs are sensitive bearer credentials. RM keeps them out of `config.json` and stores them in Windows Credential Manager, alongside the device key. Never share a webhook URL publicly; rotate it in Discord if it is exposed.
>
> If you are planning to contribute, please read the [commit guide](CONVENTIONAL_COMMITS.md) and [contributing guide](CONTRIBUTING.md) before opening a pull request. Changes involving cookie handling, encryption, storage, or process control require additional review and should be explicitly mentioned in the PR description.

> [!CAUTION]
> Never include `.ROBLOSECURITY` cookies, authentication tokens, credentials, encryption keys, or other sensitive account data in issues, pull requests, commits, logs, or screenshots. If you discover a security vulnerability, see [SECURITY.md](SECURITY.md) for how to report it privately.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Usage

1. **First launch** - Nothing to set up. Encryption configures itself on this PC
2. **Add accounts** - Click "+ Add Account" and paste your `.ROBLOSECURITY` cookie
3. **Launch** - Select an account, enter a Place ID, and click Launch
4. **Bulk launch** - Ctrl+click or Shift+click to select multiple accounts, then use the group panel
5. **Settings** - Configure multi-instance, privacy mode, auto-arrange, and more

> [!CAUTION]
> Multi-instance and privacy features interact with Roblox's local processes and files. Roblox updates may change or break these behaviours, so do not assume that a feature will continue working indefinitely.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Credits

- [RobloxManager](https://gitlab.com/centerepic/robloxmanager) by [centerepic](https://gitlab.com/centerepic) - The modern version of RobloxAccountManager that this repository was forked from
- [RobloxAccountManager](https://github.com/ic3w0lf22/Roblox-Account-Manager) by [ic3w0lf22](https://github.com/ic3w0lf22) - The original Roblox Account Manager that served as the primary reference for this project
- [Lucide Icons](https://lucide.dev/icons/) - The icon pack used for icons throughout the app. Licensed under the [ISC License](https://github.com/lucide-icons/lucide/blob/main/LICENSE).

### Contributors

<a href="https://github.com/Paryx-games/roblox-manager/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=Paryx-games/roblox-manager" alt="contributors" />
</a>

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Building From Source

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) 22+
- [pnpm](https://pnpm.io/) 11+
- Windows 10/11 (required for Win32 APIs)

### Build

```bash
# Clone the repository
git clone https://github.com/Paryx-games/roblox-manager.git
cd roblox-manager

# Install frontend dependencies
pnpm --dir ram_ui install

# Build the production Tauri application and installer
pnpm --dir ram_ui tauri build
```

The compiled binary will be at `target/release/ram_ui.exe`.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Development Commands

Run these commands from the repository root unless noted otherwise.

### Rust workspace

```powershell
# Format all Rust crates
cargo fmt --all

# Check for errors without building
cargo check

# Run with debug logging
$env:RUST_LOG="debug"; cargo run

# Run all Rust tests
cargo test --workspace

# Run the strict CI lint configuration
cargo clippy --workspace --all-targets -- -D warnings

# Build the complete workspace in release mode
cargo build --release
```

### Tauri and React UI

The Tauri UI lives in `ram_ui/` and uses `pnpm`.

```powershell
# Install frontend dependencies
pnpm --dir ram_ui install

# Start the Vite frontend only
pnpm --dir ram_ui dev

# Start the full Tauri desktop app with hot reload
pnpm --dir ram_ui tauri dev

# Check TypeScript types
pnpm --dir ram_ui typecheck

# Run ESLint and stylelint
pnpm --dir ram_ui lint

# Build the production frontend bundle
pnpm --dir ram_ui build

# Build the optimized Tauri application and installer
pnpm --dir ram_ui tauri build

# Build a debug Tauri application
pnpm --dir ram_ui tauri build --debug
```

The Tauri command wrapper selects the Windows application icon automatically. Development and debug builds use
`assets/logos/Development.ico`; versions containing `-alpha` use `Alpha.ico`; versions containing `-beta` use
`Beta.ico`; and release candidates and stable versions use `Live.ico`. The version is read from the root
`Cargo.toml`.

> [!TIP]
> For the quickest local feedback, run `cargo check` and `pnpm --dir ram_ui typecheck` while developing. Use the full validation commands before opening a pull request.

> [!IMPORTANT]
> Changes under `ram_core/` and the legacy egui UI require special care. The current v2 migration is focused on the Tauri shell and React frontend; do not rewrite core or legacy UI code as part of an unrelated frontend change.

### Pull request validation

Run the same checks used by CI before submitting a change:

```powershell
cargo fmt --all -- --check
cargo check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
pnpm --dir ram_ui lint
pnpm --dir ram_ui typecheck
```

> [!NOTE]
> Use [Conventional Commits](CONVENTIONAL_COMMITS.md) for commit messages. User-facing changes should also be added to the `Unreleased` section of [CHANGELOG.md](CHANGELOG.md).

> [!WARNING]
> Never include Roblox cookies, authentication tokens, passwords, webhook URLs, account data, logs, or build artifacts in commits, issues, pull requests, or screenshots. Report security vulnerabilities privately through [SECURITY.md](SECURITY.md).

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Reporting Issues & Requesting Features

Found a bug or have an idea for RM? Here's where it goes:

- **Bug report** - [Open a bug report](https://github.com/Paryx-games/roblox-manager/issues/new?template=bug_report.yml) using the bug template. Include your RM version, OS build, and repro steps.
- **Feature request** - [Open a feature request](https://github.com/Paryx-games/roblox-manager/issues/new?template=feature_request.yml) using the feature template. Explain the use case, not just the feature.
- **Security vulnerability** - Do not open a public issue. Follow the private disclosure process in [SECURITY.md](SECURITY.md) instead.
- **Existing issues** - Check the [open issues](https://github.com/Paryx-games/roblox-manager/issues) first in case it's already tracked.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Contributing

Contributions are what make the open source community such an amazing place to learn, inspire, and create. Any contributions you make are **greatly appreciated**.

If you have a suggestion that would make this better, please fork the repo and create a pull request. You can also simply open an issue with the tag "enhancement". Read the [commit guide](CONVENTIONAL_COMMITS.md) and [contributing guide](CONTRIBUTING.md) first - PRs touching cookie handling, encryption, storage, or process control need extra review.

1. Fork the Project
2. Create your Feature Branch (`git checkout -b feature/AmazingFeature`)
3. Commit your Changes (`git commit -m 'feat: add some amazing feature'`)
4. Push to the Branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## License

[MIT](LICENSE)

<p align="right">(<a href="#readme-top">back to top</a>)</p>
