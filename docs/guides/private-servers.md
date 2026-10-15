---
description: Save and launch Roblox private-server links from RM.
icon: lock
---

# Private servers

The **Private Servers** workspace keeps server bookmarks with the account and game information needed to return to them.

## Add a server

1. Open **Private Servers**.
2. Choose **Add server**, enter a recognisable **Name**, and paste the Roblox private-server link in **URL**.
3. Choose **Add Server** and wait for resolution.
4. Review the saved bookmark's game and available server information.

RM can resolve supported share links and store the resulting server target for later launches. A saved server may also show its game icon after the icon lookup completes.

## Launch a bookmark

Use the account picker on a saved server row to choose one or several accounts, then choose **Launch**. Each account still needs valid credentials and permission to join the destination. Launch pacing applies to multiple accounts.

Search or sort saved servers by name, recency or status. The row's menu offers link copying, editing and removal. Editing a bookmark does not change the underlying Roblox server. Copying a private link is an explicit clipboard action; do not share it accidentally.

## Check links and ownership

The page checks saved links with available managed accounts when it opens. Choose **Refresh status** to reload bookmarks and retry checks, or use **Check link and ownership** in one row's menu. **Stop checking** stops queued checks; an in-progress request may still finish.

The link icon shows whether Roblox resolved or rejected the link, or could not verify it. The shield separately shows ownership: **green** means the owner is one of the managed accounts, **yellow** means ownership is unknown or being fetched, and **red** means Roblox returned an owner who is not managed here. Available owner names link to their Roblox profiles. Failed authentication, rate limits, missing fields or inaccessible server data do not count as proof of non-ownership.

Checks use Roblox's join-page markup and accessible-private-server API. Roblox may change the markup or hide owner information. A resolved link is not a guarantee that every account can join. Missing owner data stays unknown, and the checks do not modify a server, its permissions or subscription.

{% hint style="warning" %}
Treat private-server links and access codes as private invitations. Do not post them in issues, screenshots, commits, or chat logs.
{% endhint %}

Expired links, revoked access, or Roblox API changes can prevent a bookmark from resolving. Remove the old bookmark and add a fresh link when the server owner changes access.
