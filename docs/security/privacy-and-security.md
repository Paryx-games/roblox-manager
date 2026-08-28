---
description: Protect your Roblox credentials and control local data cleanup.
icon: shield-halved
---

# Privacy and security

## Privacy and security

Roblox Manager stores account credentials so you can launch sessions quickly. Protect those credentials like passwords.

{% hint style="danger" %}
Your `.ROBLOSECURITY` cookie can access your Roblox account. Never share it with anyone.
{% endhint %}

### Your security checklist

{% columns %}
{% column %}
#### <i class="fa-lock">:lock:</i>

* `.ROBLOSECURITY` cookies
* Encryption keys
* Authentication tokens
{% endcolumn %}

{% column %}
#### <i class="fa-eye-slash">:eye-slash:</i>

* Screenshots showing cookies
* Logs containing credentials
* Credentials in issues or chat
{% endcolumn %}
{% endcolumns %}

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
### Remove the old account entry

Remove the affected account from Roblox Manager.
{% endstep %}

{% step %}
### Add the account again

Sign in again and add a fresh cookie only after revocation.
{% endstep %}
{% endstepper %}

{% hint style="success" %}
Removing and re-adding an account keeps your stored credential current.
{% endhint %}

### How account storage works

Roblox Manager encrypts account data before storing it. Cookies are not stored as plain text.

<details>

<summary>View storage protections</summary>

* **AES-256-GCM** encrypts stored account data.
* **Windows Credential Manager** can protect machine-backed keys.
* **Argon2id** supports the optional master password mode.

</details>

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
