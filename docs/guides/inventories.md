---
description: Browse and compare owned items across managed accounts.
icon: boxes-stacked
---

# Inventories

The **Inventories** workspace compares owned Roblox items across selected accounts. It is separate from developer creations and uploads in [Asset Manager](asset-manager.md).

## Open the workspace

Enable **Show Inventories** under **Settings > App and data > Visible pages**, save the change, then open **Inventories** in the navigation rail. Select accounts in the workspace account picker. Previously selected accounts are carried into the page when it opens.

## Find items

- Search by name and filter item categories such as accessories, clothing, animations and gear.
- Choose **All items**, **Shared by all** or **Unique to one** to compare the loaded accounts.
- Switch between grid and list views and adjust the item size and sort order.
- Use **Refresh** when ownership has changed or a request needs retrying. Refresh bypasses cached inventories; if you change account selection while it is loading, the new selection also loads fresh inventories.

Comparison results depend on the inventories that loaded successfully. An unavailable or restricted inventory is not proof that an account owns no items.

## Cached inventories and account previews

Accounts and Inventories share a bounded memory cache of up to 64 inventories for 15 minutes. Reopening either page reuses recent results, including item names, categories, thumbnails and available prices. Each cache entry belongs to one account and credential revision. Refresh bypasses it; accepted asset uploads, unlocking the account store and **Clear application caches** invalidate it. Cached ownership can be out of date until refreshed, and the cache is cleared when RM closes.

The Accounts page loads a compact preview automatically after selection settles. It shows up to two rows, or shared items when multiple accounts are selected. Additional items appear as skeleton frames with **Load more in inventories tab**. Open Inventories for the full grid, filters and comparisons. If the workspace is hidden, **Enable Inventories** opens Settings so you can enable it under **App and data > Visible pages**.

## Select and inspect

Select individual items, use the selection shortcuts, or drag a selection rectangle in the grid. The selection area lets you find selected items. Copy an item ID or open supported item pages through the browser controls.

Opening an account's inventory uses its account browser session. This page does not transfer, purchase or delete owned items.
