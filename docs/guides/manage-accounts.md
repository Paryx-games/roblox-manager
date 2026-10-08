---
description: Add, organize, and use your Roblox accounts.
icon: users
---

# Manage accounts

Keep all your Roblox accounts in one organized list.

Each account shows its username, display name, avatar, status, and validation state.

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

### Session history

Use **Session history** in the title bar from any workspace. The right panel
reduces the page width and stays open when you change pages. Drag its left edge,
or focus the separator and use arrow keys, Home or End to resize it.

History records observed game joins, departures, game/server changes, online,
offline, Studio and moderation changes while RM is running and the store is
unlocked. The first successful presence observation establishes a baseline;
it does not claim a new join occurred. Presence is sampled, so brief sessions
between checks may be missed. Moderation is recorded when account validation
detects it. Network failures do not count as going offline.

**Until app closes** keeps history in memory and starts empty after a restart.
**Save to encrypted file** retains the latest 10,000 events in
`session-history.dat` beside `config.json`, encrypted with the account store's
data key. A new/recovered account store cannot decrypt history from the old
store. An unreadable history is preserved until you explicitly clear it.
Switching back to memory stops writing, but leaves previously saved history
on disk. **Clear** clears memory and both the saved file and its backup.

Filter by account and choose **Export** for CSV or JSON. Exports contain user
IDs, observed times, events, game locations and available Place/Job IDs; no
credentials or account names are included. Exported activity is unencrypted,
so review it before sharing. Export uses the current account filter.

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
