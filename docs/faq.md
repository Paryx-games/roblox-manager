---
description: Answers to common questions about accounts, multi-instance, security, and troubleshooting.
icon: circle-question
---

# Frequently asked questions

Find answers to common questions about using Roblox Manager (RM), securing your credentials, running multiple accounts, and troubleshooting issues.

---

## General

<details>

<summary>What is Roblox Manager (RM)?</summary>

Roblox Manager is a native Windows desktop application built in Rust that helps you manage multiple Roblox accounts, launch game sessions, join private servers, inspect groups, upload developer assets, and tile running Roblox client windows in a clean layout.

</details>

<details>

<summary>Which operating systems are supported?</summary>

Roblox Manager is designed specifically for **Windows 10** and **Windows 11** (64-bit). It relies on native Windows APIs, Windows Credential Manager, and the Microsoft Edge WebView2 runtime. macOS and Linux are not supported.

</details>

<details>

<summary>Where are my application data and logs stored?</summary>

Saved application data normally lives in the roaming Windows AppData directory:

```text
%APPDATA%\RM\
```

This directory includes:

* `accounts.dat` - Encrypted account credentials store.
* `config.json` - Application preferences and settings.
* `presets/` - Saved game launch presets.
* `rm.<YYYY-MM-DD>.log` - Daily diagnostic logs with credential redaction.

A custom account store may be elsewhere. The default installed app is in `%LOCALAPPDATA%\Roblox Manager`, and interface WebView2 data is in `%LOCALAPPDATA%\com.paryxgames.roblox-manager`. Keys and webhook credentials live in Windows Credential Manager. See [Installer and uninstall options](getting-started/installer-options.md#what-lives-where).

</details>

---

## Accounts & Security

<details>

<summary>How do I add an account to Roblox Manager?</summary>

Open **Accounts > Add account** and choose an addition method:

1. **Browser Login (Recommended):** Choose **Log in with browser**. An embedded secure browser window will open. Sign in to Roblox normally (including 2FA/passkeys if enabled), and Roblox Manager will securely capture and encrypt your session cookie.
2. **Manual Cookie Entry:** Paste your `.ROBLOSECURITY` cookie into the cookie field and validate it.
3. **Bulk import:** Paste cookies or select a text file and review individual results.

</details>

<details>

<summary>How are my account cookies protected?</summary>

Roblox Manager never stores account cookies in plain text. It uses **Envelope Encryption** with **AES-256-GCM**:

* **Device Store Mode (Default):** The master wrapping key is stored in Windows Credential Manager. Your store automatically and securely unlocks on your Windows user account without requiring a password prompt on every startup.
* **Password Store Mode (Optional):** The wrapping key is derived from a master password of your choice using the **Argon2id** key-derivation function.

All file writes use atomic, crash-safe persistence (`.tmp-*` staging, `.bak` backup, and atomic replace) to prevent data corruption during unexpected shutdowns.

</details>

<details>

<summary>What happens if I forget my master password in Password Mode?</summary>

{% hint style="danger" %}
A password-mode store requires its master password. RM does not have a password-recovery bypass.
{% endhint %}

Without the correct password, RM cannot decrypt a password-mode store. The guarded **Start over** flow requires confirmation and preserves encrypted recovery copies before replacing the active store. Those copies still require the original password. Do not use start-over for a temporary request failure, and do not test it against your only store.

</details>

<details>

<summary>Why does an account show "Invalid Cookie" or "Session Expired"?</summary>

A saved session may expire or be revoked by Roblox. Use **Revalidate account** to check it, or **Refresh accounts** to check every managed account. A forbidden request, challenge, rate limit or network error alone is inconclusive; it should not mark a previously valid credential invalid.

Use **Replace account credential**, or re-add the same account while its existing entry remains in the list. This updates the credential and preserves alias, group, pin and ordering. Removing it first deletes that organisation metadata. Removing an RM entry does not revoke the Roblox session.

</details>

<details>

<summary>Are my passwords or cookies stored in log files?</summary>

No. Roblox Manager includes an automated redaction system that scrubs `.ROBLOSECURITY` cookies, session tickets, CSRF tokens, and user home directory paths before writing any log entries to disk.

</details>

---

## Multi-Instance & Game Launching

<details>

<summary>How does Multi-Instance work?</summary>

By default, the official Roblox desktop client allows only one running instance at a time using Windows singleton objects. When Multi-Instance is enabled, Roblox Manager reserves `ROBLOX_singletonMutex` and the legacy `ROBLOX_singletonEvent` name for the lifetime of the manager process. The legacy name can refer to a mutex or an existing event; RM retains an existing event without signalling or resetting it. RM only holds handles in its own process and never closes handles inside Roblox. Failed setup releases partial acquisitions so it can be retried.

</details>

<details>

<summary>Is using Multi-Instance safe, and can I get banned?</summary>

{% hint style="warning" %}
Multi-instance interacts with Roblox's local client process handles. Roblox updates and anti-cheat systems (such as Hyperion) can change client internals without notice. Use multi-instance at your own discretion and risk.
{% endhint %}

Roblox Manager does not inject cheats, modify memory scripts, or alter game execution code. However, Roblox Corporation does not officially endorse third-party launchers or multi-client setups.

</details>

<details>

<summary>Why did my second Roblox client fail to open or close immediately?</summary>

If a second client does not open or closes right away:

1. Verify that **Multi-Instance** is toggled on in **Settings**.
2. Make sure you launch each instance through Roblox Manager rather than opening Roblox directly from a browser or desktop shortcut.
3. Ensure both Roblox Manager and Roblox are running under the same Windows privilege level (do not run one as Administrator and the other as a standard user).
4. Restart Roblox Manager and close any lingering `RobloxPlayerBeta.exe` processes in Windows Task Manager.

</details>

<details>

<summary>How do I launch multiple accounts into the same game or server?</summary>

1. Hold `Ctrl` or `Shift` and click multiple accounts in the account list to select them.
2. Using **Bulk launch** in Accounts, specify the **Place ID** (and optionally a **Job ID** or private server link).
3. Review the target and start the launch. Roblox Manager will launch the accounts in sequence with a configurable launch delay to ensure each client initializes cleanly.

</details>

<details>

<summary>What is the difference between Place ID, Job ID, and Access Code?</summary>

* **Place ID:** The unique numerical identifier of the Roblox game/place (found in the game's URL).
* **Job ID:** The unique GUID representing a specific active public server instance. Providing a Job ID connects your accounts to that exact server instance.
* **Access Code / Link:** The private server code or VIP link used to join a reserved private server.

</details>

---

## Window Management & Privacy

<details>

<summary>How does Auto Window Tiling work?</summary>

Roblox Manager queries the dimensions and work area of your active monitor(s) and automatically arranges running Roblox client windows into an organized grid layout upon launch. You can configure grid columns, rows, spacing, and target monitor in **Settings**.

</details>

<details>

<summary>What is Privacy Mode?</summary>

Privacy Mode helps reduce local browser tracking and file association between different Roblox accounts. When enabled, Roblox Manager can automatically clear `%LOCALAPPDATA%\Roblox\LocalStorage\RobloxCookies.dat` and selected local user-state folders according to your cleanup scope. Exit cleanup runs only when no Roblox client remains open.

{% hint style="info" %}
Privacy Mode manages local device artifacts. It does not alter server-side account relationships or network IP addresses.
{% endhint %}

</details>

---

## Troubleshooting & Diagnostics

<details>

<summary>Why is Browser Login not opening?</summary>

The Tauri interface and login window require **Microsoft Edge WebView2 Runtime**. The installer can set it up when missing, with an internet connection; the direct executable cannot bootstrap it. Retry with the current build and review scrubbed logs if login fails. A closed or failed login flow is not evidence that a credential was saved: confirm the account appears in the list.

</details>

<details>

<summary>How do I move my accounts and presets to a new PC?</summary>

1. If you are using **Device Store Mode**, switch your store mode to **Password Mode** in Settings before moving, or plan to re-authenticate on the new device (since Windows Credential Manager keys are tied to the local machine).
2. Close RM and back up `%APPDATA%\RM\` plus any account store at a custom path. Copy the required encrypted store, config and presets to the new PC. Keep the original backup until the new setup is verified.
3. Install Roblox Manager and open the app.

</details>

<details>

<summary>Where can I report a bug or request a feature?</summary>

* **Bug reports & feature requests:** Open an issue on [GitHub Issues](https://github.com/Paryx-games/roblox-manager/issues).
* **Security vulnerabilities:** Follow our [Security Policy](https://github.com/Paryx-games/roblox-manager/security/advisories/new) to submit security reports privately.
* **Community discussion:** Visit the [RM Website](https://paryx-games.github.io/roblox-manager/).

</details>

## Installation and interface recovery

### What does uninstall remove?

Application files and shortcuts are removed. Browser/cache cleanup and diagnostic-log removal are optional and off by default. Encrypted stores, settings, presets, recovery copies, Credential Manager keys and the shared WebView2 Runtime remain. Read [Installer and uninstall options](getting-started/installer-options.md) before manually deleting data.

### What should I do with a black or blank interface?

Close RM including its tray process, then try the current build. For a local development executable, keep Vite running with `pnpm --dir ram_ui tauri dev`, or build a standalone executable with `pnpm --dir ram_ui build:debug`. A dev build expecting a server is not an independent application bundle.

The current app has isolated interface profiles and WebView2 recovery handling. If the problem persists, try reinstalling with **Reset interface browser data**, which keeps encrypted accounts/settings/presets. Check WebView2 Runtime and collect scrubbed startup logs. Do not delete `accounts.dat`, use start-over or delete Credential Manager keys to fix a rendering problem.

### Why is an Instances action disabled?

Individual Kill requires an exact process match. Inferred and unmatched clients cannot be killed individually. Join server needs an identified account, valid selected launch accounts and usable destination details. See [Running instances](guides/instances.md).

### Why are Inventories and Asset Manager missing?

Enable **Show Inventories and Asset Manager** in Settings and save it. This controls both pages in the navigation rail. See [Settings](guides/settings.md).

### Are all visible advanced settings implemented?

No. Auto-launching games on startup, custom game arguments, FastFlags and automatic MAC rotation are unfinished. Starting RM with Windows and manual MAC rotation are separate actions. Do not assume saved advanced values are applied to game launches.

### What should a bug report include?

Include the version/build type, Windows version, steps to reproduce, whether it is the main interface or login window, and a small relevant extract of scrubbed diagnostics. Check the extract yourself before sharing. Never attach account stores, credentials, webhook URLs, private-server access codes or entire browser profiles.
