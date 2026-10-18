---
description: Browse a managed account's friends, inspect visible sessions, and manage incoming requests.
icon: user-group
---

# Friends

The account picker, friend total and refresh time share a compact header; category counts stay in the filter buttons. Drag the divider beside the friend details sidebar to resize it, or focus the divider and use Left/Right and Home/End. Its width is limited to keep the list usable, and the details stack below the list when space is narrow.

**Open profile in browser**, including from the Groups tab, opens an isolated popup signed in as the selected managed account. Mutual friends uses Roblox's paginated connection lists and temporary browser context. If Roblox requires verification or hides a connection list, use **Open profile to check access** and retry. RM cannot override those access restrictions.

Open **Friends** in the navigation rail and choose a managed account. RM remembers the selection on this installation. The header offers **Refresh**, the complete loaded friend count, distinct presence counts and the last successful presence refresh time.

## Browse and select

Search by display name or username without making network requests. Filter by **All**, **Playing**, **In Studio**, **Online**, **Offline** or **Requests**. Online means online without playing or using Studio. Missing or unfamiliar presence values show **Unavailable** and are counted separately, never as offline.

Presence uses Roblox's color scheme: green for Playing, orange for In Studio, blue for Online and gray for Offline. Game/place rows show the game's icon when Roblox provides one, even when the game-name lookup is unavailable. Studio and online activity use the Roblox Studio and Roblox application logos. Missing or failed game thumbnails fall back to the Roblox logo.

Sort by online status first, display name or username in either direction. The compact list shows avatars, visible activity, game/place information and any public Job ID Roblox provides. Job IDs use a truncated monospace field; hover for the full value or use the copy button. A missing server ID is not filled from another friend or an invitation code.

Click a name or row to open its details. Check friends individually or use the header checkbox to select all visible friends. Selection survives filtering; account changes clear it and refreshed lists remove selections for former friends. The selection toolbar copies usernames or user IDs. It never launches clients or changes friendships in bulk.

## Profile details

The right panel loads extended information only for the selected friend. **Details** shows profile text and known IDs, **Mutual friends** intersects the two visible friend lists, and **Groups** shows visible group memberships and roles. Privacy restrictions or failed lookups show an error with Retry instead of invented zero counts.

The three-dot menu supports copying identifiers, viewing the profile panel, opening the Roblox profile, joining eligible games, removing a friend and blocking a user. Remove, block and decline actions require confirmation. Failed or unconfirmed mutations keep the current list visible and explain how to retry safely.

Blocking uses a temporary browser context issued by Roblox, shared with RM's existing settings integration. If Roblox cannot provide it, the action stops with an error. RM never fabricates or persists that context. Blocking also removes an existing friendship on Roblox; unblocking does not restore it.

## Join a friend

Playing friends offer **Join**. Choose one managed account in the dialog. Expired, restricted and self-target accounts cannot be used. The dialog follows the saved **Multi-instance** setting; that setting cannot be overridden per launch here. With Multi-instance disabled, when Roblox is already running, or when retrying an uncertain launch, RM asks for confirmation because Roblox may reuse a client or disconnect another session for this account. The shared launch queue also checks for running or pending clients before an unconfirmed launch. Friends never closes clients itself.

RM rechecks the selected account's access to the friend's live presence and uses the shared launch queue, pacing and authentication. **Join same server** also checks that the visible Job ID still matches; if the friend moved, refresh before retrying. **Join in new instance** requires Multi-instance to remain enabled while the launch is queued.

A hidden Job ID can still be checked using another managed account, but RM cannot join if Roblox does not expose an accessible place and server. The result says **Launch requested**; check Instances to confirm game entry. Studio sessions and ordinary online/offline presence do not offer joining.

## Requests

**Requests** loads incoming friend requests on demand. Use **Accept** or confirm **Decline**, then RM refreshes the requests and friend counts. **Load more requests** follows Roblox's pagination; search covers the requests loaded so far. A count with **+** means more pages remain.

Outgoing request lists and cancellation are unavailable through the documented APIs used here. Messaging, Studio joining, friend-since dates, last-online timestamps, server region and server player counts are also unavailable. These are omitted or clearly labelled, rather than fabricated.

## Refresh and errors

Friend/presence snapshots cache in memory for 30 seconds, incoming pages for 30 seconds and selected profile data for five minutes. Public avatar, game-name and thumbnail requests reuse RM's shared metadata caches. Switching accounts ignores stale results, and visited workspaces retain loaded data. Friends does not continuously poll inactive accounts; use Refresh for a current snapshot.

Previously loaded data stays visible during refresh failures with an error. Missing presence or metadata shows a warning and an unavailable state. Rate limits and authentication/permission failures have distinct messages. Check the account in Accounts for login failures, and wait before retrying rate limits or challenges. RM does not bypass Roblox restrictions.

Backend caches are bounded, keyed by the viewing account and credential revision, cleared on unlock/cache clearing, and discarded on exit. Credentials and private invitation/access codes never enter the Friends responses.

## Discord notifications

Under **Settings > Integrations**, enable and route friendship events to saved Discord webhooks. RM can notify about incoming requests, added/removed friends, and successful request, decline and block actions made in RM. Incoming requests and friendship changes use complete background checks without opening Friends. Existing relationships form a silent baseline, and successful actions are reconciled so the next check does not repeat them. Each event starts disabled. See [Discord notification settings](settings.md#discord-notifications) for timing and routing.

API contracts: [Roblox Friends API](https://create.roblox.com/docs/cloud/reference/domains/friends), [Presence API](https://create.roblox.com/docs/cloud/reference/domains/presence), [Users API](https://create.roblox.com/docs/cloud/reference/domains/users), [Games API](https://create.roblox.com/docs/cloud/reference/domains/games), and [Groups API](https://create.roblox.com/docs/cloud/reference/domains/groups). Friend mutations use the authenticated routes listed in Roblox's [cookie API update](https://devforum.roblox.com/t/roblox-data-stewardship-principles-cookie-based-api-update/4579220).
