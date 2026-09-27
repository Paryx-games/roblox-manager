---
description: Download Roblox Manager and prepare it for your first session.
icon: download
---

# Installing

## Install Roblox Manager

### Requirements

You need:

* Windows 10 or Windows 11
* A working Roblox installation

macOS and Linux are not currently supported.

### Download

Download Roblox Manager from the project’s GitHub releases.

<a href="https://github.com/Paryx-games/roblox-manager/releases/latest" class="button primary" data-icon="download">Download latest release</a>

Choose the Windows x64 installer (`-setup.exe`) for a normal installation, or the portable executable to run Roblox Manager without installing it.

The installer installs Roblox Manager for your current Windows user. It includes the Microsoft WebView2 bootstrapper and installs the runtime if it is missing; this requires an internet connection. The portable executable requires WebView2 Runtime to be installed already.

### First launch

Roblox Manager configures encrypted storage during its first launch. You can then add your first account.

### Keep the app updated

Download the latest release when an update is available. Updates may restore compatibility after Roblox client changes.

Close Roblox Manager and run the newer installer. Saved accounts, settings and presets remain available. The installer prevents installing an older version over a newer one.

If an earlier installation appears as **RM** in Windows Settings, uninstall it before running the renamed Roblox Manager installer. The installer will prompt you if it detects that older installation. Your saved accounts, settings and presets are kept.

### Uninstalling

Uninstall Roblox Manager through Windows Settings. Your encrypted account store, settings and presets remain where stored, normally `%APPDATA%\RM`, so they are available if you reinstall. Custom store locations are also left untouched. Back up your data before manually removing it.

The uninstall checkbox only removes the Tauri interface's browser data. It does not remove saved accounts, settings, presets or the shared Microsoft WebView2 Runtime.

{% hint style="info" %}
Continue with [first-session.md](first-session.md "mention") after installation.
{% endhint %}
