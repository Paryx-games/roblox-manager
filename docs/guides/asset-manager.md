---
description: Upload developer assets and track moderation and permissions.
icon: upload
---

# Asset manager

The optional **Asset Manager** workspace keeps developer creations, staged files, upload progress, moderation state, thumbnails and permission results together.

## Enable the workspace

Open **Settings > App and data > Visible pages**, enable **Show Asset Manager**, and save. Inventories has its own **Show Inventories** switch. The separate Clear Cache shortcut switch does not control these pages.

## Upload an asset

1. Choose the acting Roblox account.
2. Open **Import Queue** and stage supported local files.
3. Review each row's name, asset type and account or group creator. Editing the type does not convert the source file.
4. Select the queued rows to upload and, where applicable, the target experience.
5. Choose **Upload selected**, review the confirmation, and watch upload, operation and moderation results.

RM polls pending operations and moderation results in the background. A failed upload keeps its error state so it can be investigated without blocking the rest of the workspace.

## Permissions and inventory

Use **Library** to inspect live creations for the acting account or selected group creator. Refresh creations, copy IDs, or select existing assets and an experience before **Grant selected access**. Review granted and refused results separately. Owned avatar items are in [Inventories](inventories.md), not this library.

Failed queue rows can be retried or removed; finished rows can be cleared. Retrying does not undo an asset already created on Roblox. Upload confirmation matters because uploads create permanent assets and submit them to moderation.

{% hint style="warning" %}
Asset uploads require a valid acting account and appropriate Roblox permissions for the selected creator and experience. Do not upload files containing credentials or private data. Supported types and service availability can change with Roblox APIs.
{% endhint %}
