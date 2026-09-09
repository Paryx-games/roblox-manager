# Roblox API Reference (Cookie Auth) - Contributor Guide

This doc lists the Roblox legacy web API domains that support `.ROBLOSECURITY` cookie auth, and the endpoints RM (Roblox Manager) is likely to call.

Each section is marked:

- **Verified** - fetched directly from the live docs, endpoint list and shapes confirmed as of the date shown.
- **Unverified** - based on well-known/long-standing legacy behavior, not re-checked against current docs. Confirm before shipping.

Roblox reshuffles legacy endpoints without changelogs, so treat unverified sections as "probably right, check before you build."

> **official reference:** <https://create.roblox.com/docs/en-us/cloud/reference> (filter by domain or feature)
> **llm-readable index:** <https://create.roblox.com/docs/cloud/llms.txt>

## Auth basics

All endpoints below accept the `.ROBLOSECURITY` cookie. Roblox's own docs mark cookie auth as **not recommended for production** - these are legacy/unversioned APIs that can break without a deprecation notice.

All production API calls in RM go through `ram_core` (Rust). Other languages below are testing/scratch only - never wire a real feature to anything but the Rust backend.

```rust
// cargo add reqwest --features json,cookies
// cargo add serde_json

use reqwest::{Client, cookie::Jar, Url};
use std::sync::Arc;

let jar = Arc::new(Jar::default());
let url = "https://roblox.com".parse::<Url>().unwrap();
jar.add_cookie_str(&format!(".ROBLOSECURITY={COOKIE}"), &url);

let client = Client::builder()
    .cookie_provider(jar)
    .build()?;
```

**CSRF note:** any state-changing call (POST/PATCH/DELETE) will 403 on the first try and return an `x-csrf-token` header - grab it and retry with that header set. Applies across every domain below.

```rust
// shared helper: retries once with the csrf token roblox hands back on the first 403
async fn post_with_csrf(
    client: &Client,
    url: &str,
    body: &serde_json::Value,
) -> reqwest::Result<reqwest::Response> {
    let first = client.post(url).json(body).send().await?;

    if first.status() != reqwest::StatusCode::FORBIDDEN {
        return Ok(first);
    }

    let Some(token) = first.headers().get("x-csrf-token").cloned() else {
        return Ok(first);
    };

    client.post(url).header("x-csrf-token", token).json(body).send().await
}
```

<details>
<summary>Python - testing/scratch only, not for production paths</summary>

```python
# uv add requests
import requests

session = requests.Session()
session.cookies[".ROBLOSECURITY"] = COOKIE

# most POST/PATCH/DELETE calls also need an X-CSRF-TOKEN header,
# fetched by making one request without it and reading the token
# roblox returns in the 403 response headers
```

</details>

<details>
<summary>JavaScript - testing/scratch only, not for production paths</summary>

```javascript
// pnpm add node-fetch (or use native fetch on node 18+)
const res = await fetch("https://groups.roblox.com/v1/groups/123", {
  headers: { Cookie: `.ROBLOSECURITY=${COOKIE}` },
});
```

</details>

<details>
<summary>C++ - testing/scratch only, not for production paths</summary>

```cpp
// libcurl example - manual cookie header, no session abstraction
CURL* curl = curl_easy_init();
curl_easy_setopt(curl, CURLOPT_URL, "https://groups.roblox.com/v1/groups/123");
curl_easy_setopt(curl, CURLOPT_COOKIE, (".ROBLOSECURITY=" + cookie).c_str());
```

</details>

---

## Groups - `groups.roblox.com` [Verified Sept 2026]

Manages Roblox groups: membership, roles, permissions, payouts, relationships, and moderation. This is RM's biggest surface - 86 endpoints across v1/v2. Full list: <https://create.roblox.com/docs/en-us/cloud/reference/domains/groups.md>

**Most relevant for RM (rank/member management):**

- `GET /v1/groups/{groupId}` - group info (name, description, owner, member count)
- `GET /v1/groups/{groupId}/roles` - list rolesets
- `GET /v1/groups/{groupId}/users` - list members, paginated
- `GET /v1/groups/{groupId}/roles/{roleSetId}/users` - members in a specific role
- `POST /v1/groups/{groupId}/users` - join a group
- `GET /v1/groups/{groupId}/join-requests` - pending join requests
- `POST /v1/groups/{groupId}/join-requests/users/{userId}` - accept a join request
- `DELETE /v1/groups/{groupId}/join-requests/users/{userId}` - decline a join request
- `POST /v1/groups/{groupId}/bans/{userId}` - ban a member
- `DELETE /v1/groups/{groupId}/bans/{userId}` - unban a member
- `GET /v1/groups/{groupId}/audit-log` - audit log (needs `viewAuditLogs` permission)
- `GET /v1/groups/{groupId}/membership` - authenticated user's role + permissions in the group
- `GET /v1/users/{userId}/groups/roles` - all groups a user belongs to + role in each
- `POST /v1/groups/{groupId}/payouts` - one-time Robux payout to members
- `POST /v1/groups/{groupId}/payouts/recurring` - set recurring payout splits
- `PATCH /v1/groups/{groupId}/roles/{roleSetId}/permissions` - update a roleset's permissions
- `POST /v1/groups/{groupId}/rolesets/create` - create a new roleset
- `POST /v1/groups/{groupId}/change-owner` - transfer group ownership
- `POST /v1/groups/{groupId}/claim-ownership` - claim an ownerless group

**Rank changes:** the classic `PATCH /v1/groups/{groupId}/users/{userId}` rank-change endpoint wasn't present in the current v1/v2 listing pulled - it may have moved or been folded into membership management. **Confirm exact shape against live docs before wiring this up**, don't assume the old payload still works.

**Also available, less commonly needed:** relationships (allies/enemies), social links, blocked keywords, community tiers, name history, group settings, group creation, group search.

```rust
#[derive(serde::Deserialize)]
struct GroupInfo {
    id: u64,
    name: String,
    #[serde(rename = "memberCount")]
    member_count: u64,
}

let group: GroupInfo = client
    .get(format!("https://groups.roblox.com/v1/groups/{group_id}"))
    .send()
    .await?
    .json()
    .await?;
```

---

## Users - `users.roblox.com` [Verified Sept 2026]

Core identity domain: profile lookups, display names, birthdate/gender settings, username history.

- `GET /v1/users/authenticated` - who-am-i for the current cookie
- `GET /v1/users/{userId}` - detailed user info by ID
- `POST /v1/users` - bulk lookup by ID
- `POST /v1/usernames/users` - bulk lookup by username
- `GET /v1/users/search` - search users by keyword
- `GET /v1/users/{userId}/username-history` - past usernames for a user
- `PATCH /v1/users/{userId}/display-names` - set display name for the authenticated user
- `GET /v1/users/{userId}/display-names/validate` - validate a display name change
- `GET /v1/display-names/validate` - validate a display name for a new user
- `GET` / `POST /v1/birthdate` - get/update the authenticated user's birthdate
- `GET` / `POST /v1/gender` - get/update the authenticated user's gender
- `GET /v1/description` / `POST /v1/description` - get/update profile description
- `GET /v1/users/authenticated/age-bracket` - age bracket of the authenticated user
- `GET /v1/users/authenticated/country-code` - country code of the authenticated user
- `GET /v1/users/authenticated/roles` - public roles (e.g. `"BetaTester"`) for the authenticated user

```rust
#[derive(serde::Deserialize)]
struct WhoAmI {
    id: u64,
    name: String,
    #[serde(rename = "displayName")]
    display_name: String,
}

let me: WhoAmI = client
    .get("https://users.roblox.com/v1/users/authenticated")
    .send()
    .await?
    .json()
    .await?;
```

---

## Economy - `economy.roblox.com` [Verified Sept 2026]

**Down to a single legacy endpoint** - most economy functionality (transactions, resale, purchases) has moved to Open Cloud (`apis.roblox.com`, API key/OAuth only) and is no longer on this cookie-auth domain. Don't build against old `/v2/users/{userId}/transaction-totals` or reseller paths from older guides - re-check Open Cloud endpoints instead if you need transaction history.

- `GET /v1/user/currency` - Robux balance for the authenticated user (`{ "robux": 0 }`)

```rust
#[derive(serde::Deserialize)]
struct Currency {
    robux: u64,
}

let balance: Currency = client
    .get("https://economy.roblox.com/v1/user/currency")
    .send()
    .await?
    .json()
    .await?;
```

---

## Contacts - `contacts.roblox.com` [Verified Sept 2026]

Small domain for user tags (the nickname you can set for a friend/contact).

- `POST /v1/user/get-tags` - bulk-get tags you've set for other users
- `POST /v1/user/tag` - set a tag for a user
- `GET /v1/user/tag/validate` - validate a tag string before setting it (checks moderation/length)

```rust
let body = serde_json::json!({ "targetUserId": target_id });

let resp = post_with_csrf(
    &client,
    "https://contacts.roblox.com/v1/user/tag",
    &body,
).await?;
```

---

## Account & auth [Unverified]

### `auth.roblox.com`

- `POST /v2/login` - username/password login (returns cookie)
- `POST /v2/logout` - invalidate current session
- `GET /v1/users/authenticated` - who-am-i check (also on `users.roblox.com`, verified there)

### `accountsettings.roblox.com`

- `POST /v1/email` - change account email
- `POST /v1/password` - change password (requires **current password**, not just cookie)
- `GET` / `POST /v1/birthdate` - get/set birthdate (legacy alt path - `users.roblox.com` version is verified)
- `GET /v1/email` - current email verification status

### `accountinformation.roblox.com`

- `GET /v1/phone` - phone number status
- `GET /v1/birthdate` - birthdate (alt path used by some clients)

### `twostepverification.roblox.com`

- `GET /v1/users/{userId}/configuration` - 2FA status
- `POST /v1/users/{userId}/challenges/authenticator/verify` - verify 2FA challenge during login/password change

---

## Social [Unverified]

### `friends.roblox.com`

- `GET /v1/users/{userId}/friends` - friend list
- `GET /v1/my/friends/requests` - pending friend requests
- `POST /v1/users/{userId}/request-friendship` - send friend request

### `presence.roblox.com`

- `POST /v1/presence/users` - bulk presence/online-status lookup
- `POST /v1/presence/last-online` - last-online timestamps

### `privatemessages.roblox.com`

- `GET /v1/messages` - inbox
- `POST /v1/messages/send` - send a Roblox DM

---

## Inventory & catalog [Unverified]

### `inventory.roblox.com`

- `GET /v1/users/{userId}/items/{itemType}/{itemTargetId}` - check item ownership
- `GET /v2/users/{userId}/inventory` - full inventory page

### `catalog.roblox.com`

- `GET /v1/search/items` - catalog search
- `POST /v1/catalog/items/details` - bulk item detail lookup

### `avatar.roblox.com`

- `GET /v1/users/{userId}/avatar` - full avatar/outfit data
- `GET /v1/users/{userId}/currently-wearing` - currently equipped items

---

## Game/dev side [Unverified]

### `games.roblox.com`

- `GET /v1/games` - universe info by ID
- `GET /v1/games/{universeId}/servers/{serverType}` - active servers

### `develop.roblox.com`

- `GET /v1/user/games` - games owned by the authenticated user
- `PATCH /v2/places/{placeId}` - update place config

### `badges.roblox.com`

- `GET /v1/users/{userId}/badges` - badges a user owns

---

## Utility [Unverified]

### `thumbnails.roblox.com`

- `GET /v1/users/avatar` - bulk avatar thumbnails
- `GET /v1/assets` - asset thumbnails

### `notifications.roblox.com`

- WebSocket + REST hybrid; check current docs before integrating

---

## Contributing a new endpoint integration

1. Check the live docs first - `create.roblox.com/docs/cloud/llms.txt` lists every domain's markdown reference. Legacy endpoints get renamed/removed without changelog entries.
2. Confirm cookie auth is actually supported for that endpoint - some functionality has moved to Open Cloud (API key/OAuth only), like most of `economy.roblox.com`.
3. Handle CSRF token retry logic - don't assume a single request will succeed on state-changing calls. Use `post_with_csrf` (or the equivalent for PATCH/DELETE) instead of hand-rolling it per call site.
4. Never log or commit cookies, passwords, or CSRF tokens. Use `.env` / secrets manager, never hardcode.
5. Add rate limiting on bulk/loop operations - Roblox will flag/throttle aggressive request patterns.
6. When you verify a section against live docs, flip its status to Verified with the date, and update/remove any stale endpoints it replaces.
7. All production calls live in `ram_core`. Python/JS/C++ snippets here are for one-off testing against the live API only - if it needs to ship, port it to Rust.
