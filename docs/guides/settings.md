---
description: Save preferences for launching, privacy, storage and integrations.
icon: gear
---

# Settings

Use the section navigation to find preferences. Most edits form a draft: choose **Save Settings** or **Save** in the unsaved-changes bar to apply them, or reset the draft to discard changes. Actions such as changing encryption or testing a webhook have their own controls and confirmation flows.

## Launching and window management

- **Start RM with Windows** starts the manager when your Windows user signs in.
- **Revalidate accounts on startup** checks saved credentials when RM starts.
- **Enable multi-instance** allows several Roblox clients. Close existing Roblox clients and tray processes before enabling it.
- Background-process cleanup and the kill-all confirmation preference control launch safeguards.
- Auto-arrange, target monitor, grid layout, sizing and spacing control Roblox window placement.
- Window naming identifies clients by account. Launch pacing spaces successive launches rather than starting every selected account together.

**Auto-launch on startup** launches the configured Account ID into the configured Place ID once after the store unlocks. Password mode waits for you to unlock. Startup launch verifies that account and uses the normal launch queue, privacy cleanup, custom arguments, FastFlags and window arrangement. Invalid or missing targets stop with a notice; failures are not retried automatically. Changes apply on the next RM restart. Existing configurations without a Place ID must be completed before startup launch can run. Turning off **Check accounts on startup** skips the initial roster check; the normal five-minute checks continue, and startup launch still checks its own account.

## Privacy and displayed identity

Choose the cleanup scope before launch and whether to clean on exit. Exit cleanup runs only when no Roblox client remains open. Clipboard clearing is optional and runs only after a successful RM launch while privacy cleanup is enabled. RM empties the clipboard without reading its contents; a busy clipboard produces a notice. Anonymisation changes managed account names and avatars in the interface; it does not change Roblox identities or what other players see.

**Rotate MAC address before the first launch** rotates once per RM session before authenticating the first game launch. A successful manual rotation counts for that session too. Close existing Roblox clients first; RM stops the launch if they are running. Windows may ask for administrator permission. Cancelling elevation or an unsupported adapter stops the launch; retry manually or disable rotation. RM re-enables the adapter if setting the address fails and waits up to 30 seconds for the adapter to confirm the new address. Manual rotation uses the same protections. It changes a local network setting; it is not an IP-address change or a guarantee against account association.

## App and data

**Show Inventories and Asset Manager** controls those two workspaces in the navigation rail. **Show Clear Cache in navigation** adds the separate cache shortcut; it does not control either workspace. Public avatar URLs are cached for five minutes, game and asset thumbnails for 30 minutes, group details for one minute and announcements for 30 seconds. Account inventories are cached in memory for one minute; **Refresh selected** bypasses that cache. Presence requests arriving together share a result for up to two seconds. **Clear application caches** removes these memory caches and rejects cache writes from requests already in flight; account stores and browser profiles are retained. Caches expire automatically and are discarded when RM exits. Logging controls the diagnostic level, and data-location controls show the active paths. Use the Roblox installation section to inspect the selected installation rather than assuming the default installation is always correct.

The tray keeps the manager available in the background when its main window is closed. Use the tray's exit action to finish the process rather than assuming closing the window stops background work.

## Storage and encryption

Device mode unlocks through Windows Credential Manager. Password mode requires a master password. Changing encryption affects how the encrypted store unlocks; it does not make credentials plaintext. Keep a recoverable backup before changing storage or moving PCs. See [Privacy and security](../security/privacy-and-security.md).

## Discord notifications

Configure the webhook through the integration controls, then use the explicit test action to send a test notification. The webhook URL is a credential stored in Windows Credential Manager, not ordinary configuration text. Never include it in screenshots or bug reports.

## Custom arguments and FastFlags

Custom arguments are parsed using Windows double-quote and backslash rules and passed directly to Roblox, without a command shell. Quote arguments containing spaces. RM reserves its launch URI, authentication and attribution parameters; invalid arguments stop the launch. Roblox determines which extra arguments it recognizes.

FastFlags are written before launches to `ClientSettings/ClientAppSettings.json` beside the selected `RobloxPlayerBeta.exe`, including account-specific installations. RM preserves unrelated keys and records original values in `ClientSettings/rm-original-fast-flags.json`. Removing a configured flag restores that original value at the next launch in that installation. RM owns the keys configured here; editing the same keys with another tool may be overwritten. Invalid existing JSON stops the launch instead of replacing that file. Writes are atomic and retain backups. Applied settings remain if a later authentication or launch step fails. Roblox updates may install a new directory; RM applies your flags there on the next launch.

Roblox only honors flags on its [local configuration allowlist](https://devforum.roblox.com/t/allowlist-for-local-client-configuration-via-fast-flags/3966569). Writing a flag does not guarantee the client accepts it.
