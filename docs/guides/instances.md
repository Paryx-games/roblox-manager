---
description: Inspect running Roblox clients, focus windows, and join a matched server.
icon: window-maximize
---

# Running instances

Open **Instances** to see running Roblox clients, including clients that Roblox Manager did not launch. The toolbar shows the running count and offers **Refresh**, **Arrange windows** and **Kill all Roblox**.

## Understand the rows

Each row shows the account, process ID (PID), Place ID, match type, launch time and available actions. Unknown values mean RM has not established that information.

| Match | Meaning | Individual Kill |
| --- | --- | --- |
| Exact | A launch token in the client's process command line identifies the managed launch. | Available, with confirmation and a further process check. |
| Inferred | Estimated from launch order rather than a verified token. | Disabled. |
| Unmatched | The client's account could not be identified. | Disabled. |

An inferred match is a guess. An unmatched row is not evidence that the account's credentials are invalid.

## Focus or arrange clients

Use **Focus** to bring a client's window forward. Use **Arrange windows** to apply the layout configured in Settings. A process that has not created a usable window may not be focusable yet.

## Join a client's server

1. Choose the valid accounts to launch in **Join server as**.
2. Find the client's row and choose **Join server**.
3. Wait for RM to resolve the destination and launch the selected accounts.

Server join requires an identified account and usable destination information. It is unavailable for unmatched clients. The running account is excluded from the accounts being launched. Private-server permissions and server availability still apply.

## Close clients

**Kill** closes one exactly matched client after confirmation. **Kill all Roblox** closes every Roblox client, including clients launched outside RM. The global action follows the **Confirm before killing all Roblox instances** preference.

Closing clients can interrupt game sessions. Use Focus if you only want to switch windows. See [Multi-instance](multi-instance.md) for launching several clients and [Settings](settings.md) for launch and layout preferences.
