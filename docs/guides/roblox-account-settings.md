---
description: Change display names and account visibility, with per-account verification and recovery.
icon: user-gear
---

# Roblox account settings

The **Roblox Settings** section in Accounts changes settings on Roblox itself. It is separate from RM preferences in [Settings](settings.md) and local launch overrides in [Client controls](client-controls.md).

## Choose accounts

Select one account in Accounts, or use `Ctrl`-click, `Shift`-click or **All accounts** for a batch. The settings section applies to the selected accounts. Each needs a usable saved login; Roblox restrictions can differ between accounts.

## Change a display name

1. Enter the desired **Display name**.
2. Choose **Verify and set display name**.
3. Review the result for each selected account.

Names need 3–20 characters, cannot include control characters, and must pass Roblox's own validation and moderation. Roblox enforces its change cooldown; a successful check does not bypass it. This changes the display name, not the username used to sign in or identify the account.

Successful changes are verified on Roblox and saved in RM. If Roblox accepted a name but RM could not save the local result, refresh the account rather than submitting the change again. In a batch, earlier successful changes are not rolled back when another account fails.

## Change online or in-game visibility

The **Online** row controls who can see the account online. **In-game** controls who can see it in an experience and join it. Choose an available audience button to apply that setting.

Roblox supplies the available choices for each account; RM does not unlock unavailable audiences. For a batch, a choice must be available to every selected account. **Mixed** means their current values differ.

The save checks permitted choices again and reads the settings back afterward. A request being sent is not proof the change was retained. Review per-account results before retrying a batch.

## Recover from a failure

| Result | Next step |
| --- | --- |
| Saved login is known to be expired | Log in to the same account again while keeping its RM entry. |
| HTTP 401 | Roblox rejected this request's login. Re-login or check settings in that account's browser. |
| Permission or security challenge | Open the account browser, review Roblox's restrictions/challenge, then retry. |
| Rate limit | Wait before retrying. Repeated clicks do not avoid Roblox's limit. |
| Service or connection failure | Retry later; this alone does not establish that the login expired. |
| Update sent but verification failed | Reload settings and inspect the current value before submitting again. |

Use **Retry loading settings** after correcting the cause. An account refresh can complete while finding rejected credentials, so check its result and validation state. Visibility errors alone do not change that state.

Re-adding the same account or using **Replace account credential** preserves its alias, group, pin and order. Removing it first loses that organisation metadata. See [Manage accounts](manage-accounts.md#replace-credentials-without-losing-organisation).
