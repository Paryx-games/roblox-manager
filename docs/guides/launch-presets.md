---
description: Save places, server IDs, and launch data for repeat sessions.
icon: bookmark
---

# Save launch presets

Presets store a launch target so you do not have to re-enter the same values for every account. The same presets are available in single-account and bulk-launch views.

## Create a preset

1. Open the **Presets** tab.
2. Enter a name that identifies the destination.
3. Enter the numeric **Place ID**.
4. Optionally enter a **Job ID** to target one server.
5. Optionally enter launch **Data**, such as `?vip=true`.
6. Select **Add Preset**.

The preset is saved as its own file in RM's presets folder. Use **Open folder** on the Presets tab to view the files in Explorer.

## Use a preset

Select a preset chip in the single-launch or bulk-launch panel. RM fills the Place ID, Job ID, and launch data fields for you. Review the selected account and target before launching.

## Edit or delete a preset

Use the edit control beside a saved preset, change its values, and choose **Save changes**. Use the delete control to remove it.

{% hint style="warning" %}
Launch data is validated before saving. Keep the Place ID in its dedicated field, and use Job ID for a server GUID instead of putting either value in launch data.
{% endhint %}