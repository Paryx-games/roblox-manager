---
description: Find Roblox groups and manage selected-account memberships.
icon: users
---

# Groups

The **Groups** tab lets you inspect public Roblox groups and, when an account is selected, review or change that account's membership.

## Find a group

1. Open **Groups**.
2. Search by group name and choose a result, or use the group-ID lookup.
3. Open the result to load its icon, title, description, owner, verification state, member count, announcement, and available public details.

Roblox's public APIs do not always expose every feed. A missing wall-post feed may be reported as unavailable rather than shown as an empty feed.

## Review selected accounts

Select accounts in the sidebar before opening the group membership area. RM shows each selected account's role and current membership state.

## Join or leave

Use the membership action for the loaded group. RM sends the request through each account's authenticated session and reports results separately. Accounts already in the requested state are skipped.

{% hint style="warning" %}
Roblox may require an interactive security challenge for membership changes. Complete the challenge on the linked Roblox group page, then retry the action.
{% endhint %}

Group data and membership actions depend on Roblox endpoints and account permissions. A Roblox API change can affect this tab without an RM update.