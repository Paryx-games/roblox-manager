---
description: Save places, server IDs, and launch data for repeat sessions.
icon: bookmark
---

# Save launch presets

Saved presets appear first. Choose **New preset** in the header to open the compact two-column creation form, or **Refresh** to reload saved files. The info card beside **Target tags** explains how tags choose accounts at launch time. Saved rows show compact tag signs and a matching-account count; use **Edit** to change their targets.

### Target accounts by tag

When creating or editing a preset, choose **Target tags** to launch every current account matching any of those tags. These saved targets take precedence over the manual account picker and are resolved again each time you launch. Leave the tag selection empty to keep the manual picker behavior. Normal credential and moderation checks still apply.

Tag targets refer to stable IDs in your encrypted account store. Renaming a tag keeps the preset working. Deleting a referenced tag, or importing a preset whose tags do not exist in this store, stops the launch with an error. Edit the preset and remove the **Missing tag** entry or replace it with a local tag; it never silently falls back to manually selected accounts. Existing presets have no tag targets and keep their previous behavior.

Older RM versions do not support tag targets and can discard them when editing a preset. Use a version supporting tags for these presets, and retain backups before downgrading.

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

On **Presets**, choose one or several accounts in the saved preset's account picker and select **Launch**. Preset controls in Accounts can also fill the single-launch or bulk-launch destination fields. Review the selected accounts and target before launching. Invalid credentials are blocked and multiple launches follow the configured pacing.

## Edit or delete a preset

Use the edit control beside a saved preset, change its values, and choose **Save changes**. Use the delete control to remove it.

{% hint style="warning" %}
Launch data is validated before saving. Keep the Place ID in its dedicated field, and use Job ID for a server GUID instead of putting either value in launch data.
{% endhint %}
