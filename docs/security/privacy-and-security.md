---
description: Protect your Roblox credentials and control local data cleanup.
icon: shield-halved
---

# Privacy and security

Roblox Manager stores account credentials so you can launch sessions quickly. Protect those credentials like passwords.

{% hint style="danger" %}
Your `.ROBLOSECURITY` cookie can access your Roblox account. Never share it with anyone.
{% endhint %}

### Your security checklist

{% columns %}
{% column %}

#### <i class="fa-lock">:lock:</i>

- `.ROBLOSECURITY` cookies
- Discord webhook URLs
- Encryption keys
- Authentication tokens
  {% endcolumn %}

{% column %}

#### <i class="fa-eye-slash">:eye-slash:</i>

- Screenshots showing cookies
- Logs containing credentials
- Credentials in issues or chat
  {% endcolumn %}
  {% endcolumns %}

### Protect Discord webhooks

Discord webhook URLs are bearer credentials. Anyone who has a URL can post to its channel, so treat it like a password.

Roblox Manager keeps the webhook URL out of `config.json` and stores it in Windows Credential Manager under the protected `RM-Rust` service. The URL is validated before use, and it is never written to logs or included in errors. The test flow applies the `Roblox Manager` name and bundled logo before sending a confirmation message.

If a webhook URL is exposed, delete or rotate it immediately from the Discord channel's Integrations settings. Never put a real webhook URL in a screenshot, issue, commit, or chat message.

### Protect your account cookie

Roblox Manager uses a Roblox authentication cookie to add and launch accounts. Treat this value as a password.

Do not put a real cookie in GitHub issues, commits, pull requests, screenshots, logs, or Discord messages.

#### If you shared a cookie

{% stepper %}
{% step %}

### Revoke the exposed cookie

Treat the cookie as compromised. Revoke it through Roblox as soon as possible.
{% endstep %}

{% step %}

### Replace the stored credential

After revoking the exposed Roblox session, leave the account entry in RM and use **Replace account credential** or **Add account** to authenticate the same account again. This preserves its alias, group, pin and order.
{% endstep %}

{% step %}

### Confirm the new credential

Confirm the account has valid credentials and revalidate it. Removing the RM entry does not revoke a Roblox session and is not required for replacement.
{% endstep %}
{% endstepper %}

{% hint style="success" %}
Credential replacement updates the encrypted store. If you remove an account first, re-adding it does not restore its removed organisation metadata.
{% endhint %}

### How account storage works

Roblox Manager encrypts account data before storing it. Cookies are not stored as plain text.

<details>

<summary>View storage protections</summary>

- **AES-256-GCM** encrypts stored account data.
- **Windows Credential Manager** can protect machine-backed keys.
- **Argon2id** supports the optional master password mode.

</details>

### Backups and moving PCs

The default store, configuration and presets live in `%APPDATA%\RM`, unless a custom store path is configured. Atomic writes create encrypted store backups; recovery/start-over flows preserve encrypted copies. These are not plaintext credential exports.

A device-mode backup depends on its Windows Credential Manager key. Copying `accounts.dat` to another PC does not copy that key. If you need to move the store, change to password mode while it can still be unlocked, retain the password, and verify a backed-up copy before removing the original. Forgotten passwords or missing device keys cannot be bypassed by reinstalling.

### Installer cleanup and account removal

Uninstall keeps encrypted accounts, settings, presets, recovery copies and Credential Manager keys. Optional browser cleanup removes sessions/cache; optional log cleanup removes diagnostic logs. The interface-data reset during setup also leaves saved application data intact. See [Installer and uninstall options](../getting-started/installer-options.md).

Account removal changes the active encrypted store. Older encrypted backups may still contain the previous entry. Neither removal nor uninstall automatically revokes a Roblox session.

### Browser and diagnostic boundaries

The Tauri interface, isolated login window and account browser have different profile directories. Signing into the login window is an explicit account-add flow; stored credentials are handled on the Rust side rather than returned as account-list data to React.

File and console diagnostics use credential scrubbing. Review any extract before sharing it, and never upload account stores, full browser profiles, memory dumps or authentication request payloads. A login window closing successfully is not a substitute for checking the account appears in the managed list.

### Control local data with privacy mode

Privacy mode can clear selected Roblox data before launches. It can also clean up when you exit Roblox Manager.

{% tabs %}
{% tab title="Before launch" %}
Choose cleanup before starting a session. Available options can clear cookie data, local storage, and Roblox user-state folders.
{% endtab %}

{% tab title="On exit" %}
Choose cleanup when Roblox Manager closes. Cleanup only runs when no Roblox client is open.
{% endtab %}
{% endtabs %}

{% hint style="info" %}
Review your selected cleanup options before launching. Cleanup can remove local Roblox session data.
{% endhint %}

### Report a security problem

Do not post potential vulnerabilities in a public issue. Report them privately so details stay protected.

<a href="https://github.com/Paryx-games/roblox-manager/security/advisories/new" class="button primary" data-icon="shield-halved">Report a vulnerability privately</a>

### Roblox client changes

Roblox Manager works with local Roblox processes and files. Roblox updates can affect privacy cleanup and multi-instance behavior.

<a href="../guides/multi-instance.md" class="button secondary" data-icon="clone">Review multi-instance risks</a>
