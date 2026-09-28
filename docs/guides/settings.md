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

Starting the manager with Windows is distinct from automatically launching a Roblox game. See the unfinished-feature note below.

## Privacy and displayed identity

Choose the cleanup scope before launch and whether to clean on exit. Exit cleanup runs only when no Roblox client remains open. Clipboard clearing is optional. Anonymisation changes managed account names and avatars in the interface; it does not change Roblox identities or what other players see.

Manual MAC rotation has its own action and may require appropriate permissions and adapter support. It changes a local network setting; it is not an IP-address change or a guarantee against account association.

## App and data

Developer options control visibility of Inventories and Asset Manager. The current toggle label is **Show the Assets tab in Utility**. Utility visibility preferences are also retained. Logging controls the diagnostic level, and data-location controls show the active paths. Use the Roblox installation section to inspect the selected installation rather than assuming the default installation is always correct.

The tray keeps the manager available in the background when its main window is closed. Use the tray's exit action to finish the process rather than assuming closing the window stops background work.

## Storage and encryption

Device mode unlocks through Windows Credential Manager. Password mode requires a master password. Changing encryption affects how the encrypted store unlocks; it does not make credentials plaintext. Keep a recoverable backup before changing storage or moving PCs. See [Privacy and security](../security/privacy-and-security.md).

## Discord notifications

Configure the webhook through the integration controls, then use the explicit test action to send a test notification. The webhook URL is a credential stored in Windows Credential Manager, not ordinary configuration text. Never include it in screenshots or bug reports.

## Controls that are unfinished

Auto-launching a game on startup, custom game arguments, FastFlags and automatic MAC rotation have saved configuration controls but are not wired into those automatic behaviours. Do not rely on them being applied to launches. **Start RM with Windows** and the explicit manual MAC rotation action are separate features.
