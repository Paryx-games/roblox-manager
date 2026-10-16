---
description: Save preferences for launching, privacy, storage and integrations.
icon: gear
---

# Settings

Session history storage is controlled from **App actions > Session history** in the title
bar. It defaults to memory only. Choose **Save to file** to retain
activity across restarts, or **Until app closes** to start empty on the next
restart. Earlier saved history remains on disk until explicitly cleared.
See [Session history](session-history.md) for recording and export details.

Use the section navigation to find preferences. Most edits form a draft: choose **Save Settings** or **Save** in the unsaved-changes bar to apply them, or reset the draft to discard changes. Actions such as changing encryption or testing a webhook have their own controls and confirmation flows.

## Launching and window management

- **Start RM with Windows** starts the manager when your Windows user signs in.
- **Check accounts on startup** checks saved credentials when RM starts.
- **Enable multi-instance** allows several Roblox clients. Close existing Roblox clients and tray processes before enabling it.

  RM retains both Roblox singleton names for its session, including legacy versions that use an event instead of a mutex. It does not change an existing event's state or close handles inside Roblox. If setup fails, RM releases its partial acquisitions; correct the reported problem and retry saving the draft without restarting RM. An RM session that encountered this error before upgrading needs one restart to load the fix.

- **Close Roblox tray and background processes automatically** clears leftover background clients. **Confirm before killing all Roblox instances** controls confirmation for the separate close-all action; it does not enable automatic cleanup.
- Auto-arrange, target monitor, grid layout, sizing and spacing control Roblox window placement.
- Window naming identifies clients by account. Launch pacing spaces successive launches rather than starting every selected account together.

**Auto-launch on startup** launches the configured Account ID into the configured Place ID once after the store unlocks. Password mode waits for you to unlock. Startup launch verifies that account and uses the normal launch queue, privacy cleanup, custom arguments, FastFlags and window arrangement. Invalid or missing targets stop with a notice; failures are not retried automatically. Changes apply on the next RM restart. Existing configurations without a Place ID must be completed before startup launch can run. Turning off **Check accounts on startup** skips the initial roster check; the normal five-minute checks continue, and startup launch still checks its own account.

## Privacy and displayed identity

Choose the cleanup scope before launch and whether to clean on exit. Exit cleanup runs only when no Roblox client remains open. Clipboard clearing is optional and runs only after a successful RM launch while privacy cleanup is enabled. RM empties the clipboard without reading its contents; a busy clipboard produces a notice. Anonymisation changes managed account names and avatars in the interface; it does not change Roblox identities or what other players see.

**Rotate MAC address before the first launch** rotates once per RM session before authenticating the first game launch. A successful manual rotation counts for that session too. Close existing Roblox clients first; RM stops the launch if they are running. Windows may ask for administrator permission. Cancelling elevation or an unsupported adapter stops the launch; retry manually or disable rotation. RM re-enables the adapter if setting the address fails and waits up to 30 seconds for the adapter to confirm the new address. Manual rotation uses the same protections. It changes a local network setting; it is not an IP-address change or a guarantee against account association.

## App and data

**Visible pages** provides a separate **Show** switch for each workspace, including **Show Friends**, **Show Inventories** and **Show Asset Manager**. Save the draft to update the navigation rail. Settings stays accessible even when other pages are hidden. **Show Clear Cache in navigation** adds the separate cache shortcut; it does not control either workspace.

Public avatar URLs are cached for five minutes, game and asset thumbnails for 30 minutes, group details for one minute and announcements for 30 seconds.

Accounts and Inventories share up to 64 inventories cached in memory for 15 minutes, invalidated when RM accepts an asset upload. Requests for the same account share work, with up to four accounts fetched at once. **Refresh selected** and the Accounts preview's refresh action bypass that cache.

Presence requests arriving together share a result for up to two seconds.

**Clear application caches** removes these memory caches and rejects cache writes from requests already in flight; account stores and browser profiles are retained. Account caches are also cleared when the store unlocks. Caches expire automatically and are discarded when RM exits.

**Logging crates** is a searchable checkbox menu of Rust crates from the current Cargo lockfile. Selected crates use **Log level**; everything else shows warnings and errors. New debug-build settings default to Debug for `ram_core` and `rm_tauri`, equivalent to `warn,ram_core=debug,rm_tauri=debug`. Release builds retain the Info ceiling and default to Info for those crates. Existing saved levels are preserved. Save logging changes to restart RM. An explicit valid `RUST_LOG` overrides the saved selection. This controls runtime tracing, not Cargo compiler output.

Visited workspaces retain their loaded data, filters and selection during navigation, while shared background tasks update live data. Inactive workspaces are not repeatedly reloaded. Private-server ownership and link badges use a five-minute memory cache refreshed independently of page loading.

Data-location controls show the active paths. Use the Roblox installation section to inspect the selected installation rather than assuming the default installation is always correct.

The tray keeps the manager available in the background when its main window is closed. Use the tray's exit action to finish the process rather than assuming closing the window stops background work.

## Storage and encryption

Device mode unlocks through Windows Credential Manager. Password mode requires a master password. Changing encryption affects how the encrypted store unlocks; it does not make credentials plaintext. Keep a recoverable backup before changing storage or moving PCs. See [Privacy and security](../security/privacy-and-security.md).

## Discord notifications

Choose **Add webhook** under **Integrations > Discord notifications**, give the destination a name, and enter its URL. Up to 16 destinations can be saved. The URL is a credential stored in Windows Credential Manager, not ordinary configuration text. Saved URLs are never returned to the interface. Never include them in screenshots or bug reports.

**Enable Discord notifications** pauses or resumes all delivery without removing saved URLs. Each destination also has its own **Enabled** switch, plus **Test**, **Replace** and **Remove** actions. Replacing a URL preserves that destination's routing and enabled state. Test actions explicitly send a test even when automatic delivery is paused.

Event controls and destinations use two columns where space permits. Enable an event, then check every destination that should receive it. Moderation can go to several channels while batch launches go to a different channel. An event needs its own switch, the global switch, and at least one enabled destination checked before it will be sent. These preferences save immediately, independently of the main Settings draft.

Moderation and batch-launch events keep their existing defaults. **Account session expired**, **Launch request failed** and **Roblox client exited** start disabled. Expiry only fires on a new rejection during account verification. Client exits require exact attribution and include manually closed clients; they do not prove a crash. Info cards beside each option explain when it fires. RM must be running. Delivery uses a bounded in-memory queue and does not retry lost events after restart.

Notifications include a **Job ID** when RM can confirm one. Launch failures include the requested public server ID; batches include it when every request targets that same server. Client exits include the Job ID from the last confirmed instance snapshot when available. Moderation and expiry events include server details when exactly one confirmed client is tracked for the account. Unknown values and private-server invitation/access codes are omitted.

Friendship events start disabled and use the same global, event and destination switches:

- **Friend request received** detects new incoming requests in background checks.
- **Friend added** reports a request accepted in RM or a newly observed friendship.
- **Friend removed** reports a removal in RM or a friendship missing from a complete check. It cannot tell who removed it.
- **Friend request sent**, **Friend request declined** and **User blocked** report successful actions from RM. These actions outside RM cannot be reliably inferred.

When an enabled event has an active destination, RM checks the relevant lists for unlocked accounts sequentially, with a one-minute pause between rounds. Friends does not need to be open. Each account's first complete check is silent; restarting RM, changing credentials, clearing caches or re-enabling monitoring establishes a new baseline. Failed, partial and rate-limited checks keep the previous baseline. Notifications identify both accounts by public user ID, include a Roblox profile link, and respect name anonymization. A block in RM emits **User blocked** rather than a duplicate **Friend removed** for that account.

The existing webhook is retained as **Primary webhook**. Routing metadata is saved atomically alongside notification preferences, while every URL stays in its own credential entry. Older RM versions may reject the expanded preferences file and do not support extra destinations; retain the previous preferences backup before downgrading.

## Custom arguments and FastFlags

Custom arguments are parsed using Windows double-quote and backslash rules and passed directly to Roblox, without a command shell. Quote arguments containing spaces. RM reserves its launch URI, authentication and attribution parameters; invalid arguments stop the launch. Roblox determines which extra arguments it recognizes.

FastFlags are written before launches to `ClientSettings/ClientAppSettings.json` beside the selected `RobloxPlayerBeta.exe`, including account-specific installations. RM preserves unrelated keys and records original values in `ClientSettings/rm-original-fast-flags.json`. Removing a configured flag restores that original value at the next launch in that installation. RM owns the keys configured here; editing the same keys with another tool may be overwritten. Invalid existing JSON stops the launch instead of replacing that file. Writes are atomic and retain backups. Applied settings remain if a later authentication or launch step fails. Roblox updates may install a new directory; RM applies your flags there on the next launch.

Roblox only honors flags on its [local configuration allowlist](https://devforum.roblox.com/t/allowlist-for-local-client-configuration-via-fast-flags/3966569). Writing a flag does not guarantee the client accepts it.
