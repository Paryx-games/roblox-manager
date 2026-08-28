---
description: Upload developer assets and track moderation and permissions.
icon: upload
---

# Asset manager

The optional **Assets** workspace is for developer asset uploads through Roblox Open Cloud. It keeps staged files, upload progress, moderation state, thumbnails, and permission results together.

## Enable the workspace

Open **Settings** and enable the Utility area if it is hidden. Enable the Assets view inside Utility, then open **Assets**.

## Upload an asset

1. Choose the acting Roblox account.
2. Select the asset kind and a local file.
3. Select the target universe or group when prompted.
4. Stage or upload the asset.
5. Watch the row for upload, operation, and moderation status changes.

RM polls pending operations and moderation results in the background. A failed upload keeps its error state so it can be investigated without blocking the rest of the workspace.

## Permissions and inventory

Use the inventory tree to inspect assets available to the acting account. Where supported, grant an asset to a universe and review how many permissions were granted or refused.

{% hint style="warning" %}
Asset uploads require the correct Roblox account, Open Cloud access, and permissions for the selected universe or group. Do not upload files containing credentials or private data.
{% endhint %}