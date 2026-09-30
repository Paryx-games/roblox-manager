---
description: Download Roblox Manager and prepare it for your first session.
icon: download
---

# Installing

## Install Roblox Manager

### Requirements

You need:

* Windows 10 or Windows 11, 64-bit
* A working Roblox installation
* Microsoft WebView2 Runtime, which the installer can set up if missing

macOS and Linux are not currently supported.

### Download

Download Roblox Manager from the project's GitHub releases. These instructions describe the current Tauri installer; older published builds may show different screens.

<a href="https://github.com/Paryx-games/roblox-manager/releases/latest" class="button primary" data-icon="download">Download latest release</a>

Choose the Windows x64 installer (`-setup.exe`) for a normal installation, or the portable executable to run Roblox Manager without installing it.

The installer installs Roblox Manager for your current Windows user. It includes the Microsoft WebView2 bootstrapper and installs the runtime if it is missing; this requires an internet connection. The portable executable requires WebView2 Runtime to be installed already.

During setup, choose the install location, desktop and Start menu shortcuts, and whether to reset the interface browser data. The reset can help with a blank window and does not remove saved accounts, settings or presets. You can choose whether to launch Roblox Manager on the finish page.

### First launch

Roblox Manager configures encrypted storage during its first launch. You can then add your first account.

### Keep the app updated

Download the latest release when an update is available. Updates may restore compatibility after Roblox client changes.

Close Roblox Manager and run the newer installer. Saved accounts, settings and presets remain available. The installer prevents installing an older version over a newer one.

If an earlier installation appears as **RM** in Windows Settings, uninstall it before running the renamed Roblox Manager installer. The installer will prompt you if it detects that older installation. Your saved accounts, settings and presets are kept.

### Uninstalling

Uninstall Roblox Manager through Windows Settings. Your encrypted account store, settings and presets remain where stored, normally `%APPDATA%\RM`, so they are available if you reinstall. Custom store locations are also left untouched. Back up your data before manually removing it.

The uninstaller lets you remove browser sessions and cache, remove diagnostic logs, and open the saved data folder when it finishes. These choices are off by default. Saved accounts, settings, presets and the shared Microsoft WebView2 Runtime remain available.

For defaults, data paths, upgrades and cleanup details, read [Installer and uninstall options](installer-options.md).

{% hint style="info" %}
Continue with [first-session.md](first-session.md "mention") after installation.
{% endhint %}
