---
description: Add, organize, and use your Roblox accounts.
icon: users
---

# Manage accounts

Keep all your Roblox accounts in one organized list.

Each account shows its username, display name, avatar, status, and validation state.

Visibility settings require a working saved login. If an account is marked expired after refresh, log in to it again before loading or changing visibility; RM skips visibility requests for known expired logins. An HTTP 401 from visibility settings means Roblox rejected that request's login, while permission/challenge failures, rate limits and service outages have separate retry guidance. A visibility failure alone does not change the account's credential status. Refresh checks each login and updates its validation state; completion does not mean every checked login was accepted.

### Add an account

{% stepper %}
{% step %}
### Select **Add account**

Open Roblox Manager and select **Add account**.
{% endstep %}

{% step %}
### Choose an addition method

Use **Log in with browser** and complete the Roblox login, or explicitly paste a `.ROBLOSECURITY` cookie and validate it. **Bulk import** accepts pasted cookies, one per line or comma-separated, or a text file. The chooser and selected filename appear inside the dashed file box. Wait for the per-account results before closing the menu.
{% endstep %}

{% step %}
### Choose the account

The account appears in your list and is ready to launch.
{% endstep %}
{% endstepper %}

### Organize your list

Use these tools to find accounts quickly:

* **Aliases** add recognizable names.
* **Groups** organize larger collections.
* **Pins** keep frequent accounts at the top.

Search and sorting help navigate larger lists. Custom ordering is separate from automatic sort modes. Account details include available profile, presence, inventory and connection information; missing API data may need a refresh.

### Understand account status

Presence can show whether an account is offline, online, in-game, or in Studio.

Roblox Manager also tracks validation, cookie expiry, and moderation when available.

**Refresh accounts** checks every managed account, including unselected accounts. **Revalidate account** checks the current account. A network failure, rate limit, forbidden request or security challenge does not by itself establish that the saved credential is invalid; RM retains the previous validation state for inconclusive failures.

The invalid-credentials or moderation page blocks launch actions and offers recovery controls. Switching accounts resets detail scrolling and uses a short transition, disabled when reduced motion is requested.

### Replace credentials without losing organisation

Leave the existing account in the list. Use **Replace account credential**, or **Add account** and sign in to that same Roblox account again. A duplicate warning identifies the existing account; replacement preserves its alias, group, pin and position while saving the updated credential to the encrypted store.

Removing the account first is different: re-adding it creates a new entry and does not restore the removed organisation metadata.

### Select multiple accounts

Use `Ctrl`-click to add or remove individual accounts, or `Shift`-click for a range. **All accounts** above the Accounts list selects every managed account, including hidden accounts. Choose it again to clear that selection. Multi-account pickers in other workspaces offer **All accounts** for eligible accounts.

### Account tags

Open **Tags** above the Accounts sidebar to create, rename or delete tags and filter the account list. Names are unique without regard to case, limited to 40 characters, and you can keep up to 128 tags. Select **Match any** or **Match all** for a combination of tags. **Select matching accounts** replaces the current selection with the visible matches; launch controls then apply to that selection. **All accounts** still selects every managed account, including hidden accounts.

The **Tags** section on an account profile lets you add several tags without changing its group. With several accounts selected, it shows tags shared by all of them. Adding or removing a shared tag applies to all selected accounts and preserves their other tags. Renaming a tag retains assignments and preset references; deleting one removes it from all accounts, and presets using it must be edited before launching.

Tags are saved with the encrypted account store and retained during credential replacement. Existing stores load with no tags. Older RM versions ignore tag fields and can discard them if they save the store, so retain a backup before downgrading.

### Change Roblox account settings

The **Roblox Settings** section changes display names and online/in-game visibility on Roblox. It is separate from local RM preferences. For validation, per-account restrictions, batch results and recovery, see [Roblox account settings](roblox-account-settings.md).

### Session history

Open **App actions > Session history** in the title bar to review observed activity across workspaces. See [Session history](session-history.md) for retention, export, clearing and migration details. This feature requires v2.2.0 builds or later.

### Export account information

**Export accounts** downloads a CSV of account metadata, including `account_age` and `last_used`. It does not export login cookies and is not a credential-store backup. Treat usernames and account organisation as private information when sharing the file.

### Remove an account

Remove an account when you no longer want Roblox Manager to manage it.

{% hint style="warning" %}
Removing an account deletes it from Roblox Manager's active stored account list, including its organisation metadata. It does not revoke that Roblox session or close a running client. Encrypted backups or recovery files may still contain earlier store contents.
{% endhint %}

### Keep your account secure

Your `.ROBLOSECURITY` cookie acts like a password. Never share or publish it.

<a href="../security/privacy-and-security.md" class="button secondary" data-icon="shield-halved">Read privacy and security guidance</a>
