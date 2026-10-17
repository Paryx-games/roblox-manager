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

Share links can identify the experience without a place ID. RM resolves its root place before reading the invitation, using a working account rather than an expired first account.

## Launch a bookmark

Use the account picker on a saved server row to choose one or several accounts, then choose **Launch**. Each account still needs valid credentials and permission to join the destination. Launch pacing applies to multiple accounts.

Each private launch resolves a fresh UUID access code from the invitation with that account's session. Invitation codes and access codes are separate values. This also covers existing direct-link bookmarks and bookmarks whose saved access code was cleared during editing; no bookmark migration is required. If resolution fails, RM stops the launch with an error instead of substituting the invitation code for the access code.

Search or sort saved servers by name, recency or status. The row's menu offers link copying, editing and removal. Editing a bookmark does not change the underlying Roblox server. Copying a private link is an explicit clipboard action; do not share it accidentally.

## Check links and ownership

RM checks saved links in the background after accounts unlock, even while another page is open. Both badges use a memory cache for five minutes and keep their previous values during refresh. Choose **Refresh status** to queue fresh checks, or use **Check link and ownership** in one row's menu. Checks run one at a time and time out after 60 seconds per server. Account or bookmark changes invalidate cached results; locking or clearing application caches removes them. Results are discarded when RM exits.

The **left shield** shows ownership across saved accounts: **green** means at least one saved account owns the server, **red** means none owns it, and **yellow** means ownership is being checked or could not be fully verified. A red result requires either a returned owner ID that is not saved in RM, or complete owned-server checks for every saved account. Expired or unavailable accounts keep an otherwise inconclusive result yellow. Failed authentication, rate limits, missing fields and incomplete pagination do not count as proof of non-ownership.

The **right icon** independently shows invitation availability: **green** means Roblox resolved the invitation, **red** means it is inactive or invalid, and **yellow** means availability is being checked or is unknown. This is separate from ownership and the availability of an owner profile. Roblox still checks each selected account's join permission when launching. Hover over either icon for its meaning.

Checks use Roblox's join-page markup, accessible-private-server API, and each working account's owned-server list. Owned entries are matched using their invitation code rather than game or server names. Shared invitations may resolve successfully without an accessible owner profile; the row reports profile availability independently of the ownership badge. A resolved link is not a guarantee that every account can join. Checks do not modify a server, its permissions or subscription.

RM checks the accessible-server list for every working account, since another account may be able to see an owner that the first account cannot. The resolved invitation is reused across these lookups. A failed profile request keeps the resolved link status intact, and failure on one bookmark does not stop checks for the remaining bookmarks.

Each bookmark check has a 60-second limit, including Roblox retries, so an unavailable service cannot hold up the queue indefinitely. A timeout reports a retryable failed check; it does not establish non-ownership.

The owner text shows **Owner unknown**, **Owner check limited**, or **Owner check failed** when a profile is unavailable. Hover over the text or shield for details. Profiles require an owner ID returned by Roblox. A shared invitation can work even when it is absent from every checked account's accessible-server metadata; its owner's username remains unavailable in that case. Wait before retrying rate limits, and use **Refresh status** to retry failed or incomplete requests.

{% hint style="warning" %}
Treat private-server links and access codes as private invitations. Do not post them in issues, screenshots, commits, or chat logs.
{% endhint %}

Expired links, revoked access, or Roblox API changes can prevent a bookmark from resolving. Remove the old bookmark and add a fresh link when the server owner changes access.
