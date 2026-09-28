---
description: Choose shortcuts, understand upgrades, and control what uninstall removes.
icon: sliders
---

# Installer and uninstall options

This page describes the Tauri installer in the current v2 source. Older releases may have different screens. Use the release notes to check which build you downloaded.

## Choose your download

| File | What it does |
| --- | --- |
| `roblox-manager-vX.Y.Z-windows-x64-setup.exe` | Installs Roblox Manager for your Windows user and sets up WebView2 if needed. |
| `roblox-manager-vX.Y.Z-windows-x64.exe` | Runs the application directly. Requires an existing WebView2 Runtime. |
| `SHA256SUMS.txt` | Contains checksums for both executables. |

The direct executable still uses Windows application-data folders and Credential Manager. It does not keep all data beside the executable.

## Installation choices

The default installation folder is `%LOCALAPPDATA%\Roblox Manager`. Installing the application itself does not require administrator privileges. Choose another writable folder on the location page if needed.

| Option | Default | Effect |
| --- | --- | --- |
| Create desktop shortcut | On | Adds a shortcut named Roblox Manager. |
| Create Start menu shortcut | On | Adds a shortcut in the fixed Roblox Manager Start menu folder. |
| Reset interface browser data | Off | Removes the app's interface browser data. Can help with a blank interface; saved accounts, settings and presets remain. |

There is no Start menu folder selection page. On the final page, choose whether to launch the application. Changing shortcut choices during a normal interactive reinstall can remove existing shortcuts belonging to this installation.

### WebView2

Setup embeds Microsoft's WebView2 bootstrapper and runs it when the runtime is missing. The bootstrapper needs internet access to download the runtime. This is not an offline runtime installer. The runtime is shared with other applications and is not removed when Roblox Manager is uninstalled.

## Upgrade an existing installation

1. Close Roblox Manager, including its tray process.
2. Back up your encrypted store and configuration before testing a new build.
3. Run the newer installer and review its upgrade and shortcut choices.
4. Launch the application and confirm your accounts, organisation and presets remain available.

Installing an older version over a newer installation is blocked. If Windows Settings lists the previous product as **RM**, uninstall that installation first. The renamed installer detects this case and stops with instructions; it does not silently install a second copy. Saved account data remains available after the old application is uninstalled.

## Uninstall with control over cleanup

Open **Windows Settings > Apps**, select **Roblox Manager**, and choose **Uninstall**. Before confirming, choose any optional cleanup:

| Option | Default | Effect |
| --- | --- | --- |
| Remove browser sessions and cache | Off | Removes interface WebView2 data and the RM login/browse-as browser profiles. You will need fresh browser sessions. |
| Remove diagnostic logs | Off | Removes `rm.*.log` files from `%APPDATA%\RM`. |
| Open saved data folder after uninstall | Off | Opens `%APPDATA%\RM` in Explorer if the folder exists. |

Encrypted accounts, settings, presets, recovery copies and Credential Manager keys are retained. Custom account-store locations remain untouched. There is no account-data wipe checkbox. Opening the standard saved-data folder does not locate a custom store for you.

Browser cleanup is separate from removing saved account credentials. Reinstalling can still unlock the encrypted store on the same Windows user if its key remains available.

## What lives where

| Location | Contents |
| --- | --- |
| `%LOCALAPPDATA%\Roblox Manager` | Default installed application and uninstaller. |
| `%APPDATA%\RM` | Default encrypted store, configuration, presets, recovery files and diagnostic logs. |
| `%LOCALAPPDATA%\com.paryxgames.roblox-manager` | Interface WebView2 profiles, including recovery profiles. |
| `%APPDATA%\RM\tauri_login_profile` | Isolated login profile directory. |
| `%APPDATA%\RM\webview_browse_as` | Account browser profiles. |
| Windows Credential Manager | Device encryption keys and protected integration credentials. |

Do not delete the data folder or Credential Manager keys as a general troubleshooting step. An encrypted file backup alone cannot unlock a device-mode store on another PC. See [Privacy and security](../security/privacy-and-security.md).

## Unattended setup

Silent, passive and update runs skip the interactive choices. Browser and log cleanup remain off. These modes do not provide an unattended account-store reset. Installer switches are described in the [release guide](../developers/releasing.md#installer-verification).

## Before calling an installer release verified

Use a separate Windows test user or VM and backed-up test stores. Check a fresh installation, an upgrade, each shortcut choice, each uninstall cleanup option, and a machine without WebView2. A successful installer build confirms packaging, not that every interactive path has been tested.
