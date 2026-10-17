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

### Open Roblox home

Select one or more valid accounts, then choose **Open to home screen** beside **Open browser** in the launch controls. This opens the installed Roblox app signed in as each selected account, without joining a game or requiring a Place ID. **Open browser** continues to open the Roblox website.

Home launches use the same player path, launch pacing, multi-instance setting and privacy safeguards as game launches. Enable Multi-instance to keep several clients open. Home clients are tracked in Instances without a launch Place ID or Job ID.

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

Tags appear as small colored signposts arranged horizontally on account rows. Click an account's signs to check or uncheck its tags directly. One tag also shows its name beside the profile name. Up to three signs appear side by side, followed by **+N** for any additional tags. Hover over the signs to see all names. Untagged accounts reveal the tag control on hover or keyboard focus.

Choose **Manage tags...** from a tag menu to create or edit a name and color, using the same color controls as groups. Names are unique without regard to case, limited to 40 characters, and you can keep up to 128 tags. Renaming or recoloring preserves assignments and preset references; deleting a tag removes it from every account, and presets using it must be edited before launching. Existing tags without a saved color start with a neutral color.

The small tag action beside **All accounts** filters the sidebar. Its menu can match any selected tag or **Match all selected tags**. **Select matching accounts** replaces the current selection with visible matches for batch actions. **Show all accounts** clears the filter; **All accounts** still selects every managed account, including hidden accounts.

With several accounts selected, the profile tag menu shows shared tags and marks partially assigned tags as **Some selected**. Checking a partial tag applies it to every selected account. Unchecking a shared tag removes it from the selection while preserving other assignments. Tag edits never change account groups.

Tags are saved with the encrypted account store and retained during credential replacement. Existing stores load with no tags. Older RM versions ignore tag fields and can discard them if they save the store, so retain a backup before downgrading.

### Change Roblox account settings

Drag the six-dot handle on Launch, Roblox settings, Connections, Inventory, or Account details to rearrange these sections. With a handle focused, **Alt + Up/Down** moves the section using the keyboard. The layout saves automatically on this installation and applies to every account and multi-account selection. Account identity and recovery warnings stay at the top.

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
