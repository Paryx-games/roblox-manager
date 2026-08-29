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

Roblox Manager stores all application files in your local Windows AppData directory:

```text
%APPDATA%\RM\
```

This directory includes:
* `accounts.dat` — Encrypted account credentials store.
* `config.json` — Application preferences and settings.
* `presets/` — Saved game launch presets.
* `rm.<YYYY-MM-DD>.log` — Daily diagnostic log files (with sensitive credentials automatically redacted).

</details>

---

## Accounts & Security

<details>

<summary>How do I add an account to Roblox Manager?</summary>

You can add accounts using either of two methods:

1. **Browser Login (Recommended):** Click **+ Add Account** and choose **Browser Login**. An embedded secure browser window will open. Sign in to Roblox normally (including 2FA/passkeys if enabled), and Roblox Manager will securely capture and encrypt your session cookie.
2. **Manual Cookie Entry:** Click **+ Add Account**, choose **Manual Cookie**, and paste your `.ROBLOSECURITY` cookie.

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
Because Roblox Manager uses zero-knowledge encryption, your stored account data cannot be decrypted or recovered without your master password.
{% endhint %}

If you lose your master password, you will need to reset the store and re-add your accounts.

</details>

<details>

<summary>Why does an account show "Invalid Cookie" or "Session Expired"?</summary>

Roblox invalidates `.ROBLOSECURITY` cookies whenever:
* You explicitly click **Log Out** on the Roblox website in your browser.
* Your account password or security settings are changed.
* Roblox invalidates existing sessions during routine security checks or location changes.

To resolve this, click on the account in Roblox Manager, select **Re-authenticate** or **Edit**, and sign in again or supply a fresh cookie.

</details>

<details>

<summary>Are my passwords or cookies stored in log files?</summary>

No. Roblox Manager includes an automated redaction system that scrubs `.ROBLOSECURITY` cookies, session tickets, CSRF tokens, and user home directory paths before writing any log entries to disk.

</details>

---

## Multi-Instance & Game Launching

<details>

<summary>How does Multi-Instance work?</summary>

By default, the official Roblox desktop client allows only one running instance at a time using a Windows named mutex (`ROBLOX_singletonEvent`). When Multi-Instance is enabled in Roblox Manager, the application duplicates and closes this mutex handle upon launch, enabling you to run multiple Roblox clients simultaneously under different accounts.

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
2. In the **Group Launch** panel, specify the **Place ID** (and optionally a **Job ID** or private server link).
3. Click **Launch Selected**. Roblox Manager will launch the accounts in sequence with a configurable launch delay to ensure each client initializes cleanly.

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

Privacy Mode helps reduce local browser tracking and file association between different Roblox accounts. When enabled, Roblox Manager can automatically clear `%LOCALAPPDATA%\Roblox\LocalStorage\RobloxCookies.dat` and temporary user-state folders prior to game launch or when closing the manager.

{% hint style="info" %}
Privacy Mode manages local device artifacts. It does not alter server-side account relationships or network IP addresses.
{% endhint %}

</details>

---

## Troubleshooting & Diagnostics

<details>

<summary>Why is Browser Login not opening?</summary>

Browser Login requires the **Microsoft Edge WebView2 Runtime**. Windows 11 and recent updates of Windows 10 include this by default. If WebView2 is missing or corrupted on your system, download and install the *Evergreen Bootstrapper* from Microsoft's official WebView2 page.

</details>

<details>

<summary>How do I move my accounts and presets to a new PC?</summary>

1. If you are using **Device Store Mode**, switch your store mode to **Password Mode** in Settings before moving, or plan to re-authenticate on the new device (since Windows Credential Manager keys are tied to the local machine).
2. Copy the `%APPDATA%\RM\` folder to the same location on your new computer.
3. Install Roblox Manager and open the app.

</details>

<details>

<summary>Where can I report a bug or request a feature?</summary>

* **Bug reports & feature requests:** Open an issue on [GitHub Issues](https://github.com/Paryx-games/roblox-manager/issues).
* **Security vulnerabilities:** Follow our [Security Policy](https://github.com/Paryx-games/roblox-manager/security/advisories/new) to submit security reports privately.
* **Community discussion:** Visit the [RM Website](https://paryx-games.github.io/roblox-manager/).

</details>
