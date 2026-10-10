---
description: Run more than one Roblox client at the same time.
icon: clone
---

# Multi-instance

Multi-instance lets Roblox Manager run more than one Roblox client simultaneously.

Use it when you need several accounts running at once.

{% hint style="danger" %}
Multi-instance may conflict with Roblox updates or anti-cheat systems. Use it at your own risk.
{% endhint %}

### Before you enable it

Multi-instance changes local Roblox client behavior. Roblox updates can change or disable this feature.

<details>

<summary>When should I use multi-instance?</summary>

Use it only when you need several Roblox accounts open at once. Use a normal launch for a single session.

</details>

### Enable multi-instance

{% stepper %}
{% step %}
### Open **Settings**

Open Roblox Manager and go to **Settings**.
{% endstep %}

{% step %}
### Turn on multi-instance

Enable the multi-instance option.

Choose **Save Settings** or **Save** in the unsaved-changes bar to apply it. Turning on the draft checkbox alone does not enable multi-instance.

Close existing Roblox clients, including tray/background processes, first. The automatic background-process cleanup preference can help with leftover tray processes; it is separate from deliberately killing every running game.
{% endstep %}

{% step %}
### Launch your sessions

Return to your account list and launch the accounts you need.
{% endstep %}
{% endstepper %}

### Choose a launch method

{% tabs %}
{% tab title="Launch one at a time" %}
1. Select an account.
2. Launch the Roblox game.
3. Repeat for each additional account.
{% endtab %}

{% tab title="Launch in bulk" %}
1. Select several accounts with `Ctrl`-click or `Shift`-click.
2. Choose **Bulk launch** in Accounts and enter the destination.
3. Launch the selected accounts in sequence with the configured launch pacing.
{% endtab %}
{% endtabs %}

### What happens next

Each selected account opens in its own Roblox client session. You can then arrange the open windows as needed.

Use [Instances](instances.md) to focus clients and review exact, inferred or unmatched attribution. Automatic window arrangement uses Settings; **Arrange windows** applies it manually.

{% hint style="info" %}
Start with two accounts to confirm multi-instance works in your current Roblox version.
{% endhint %}

### Important limitations

Roblox Manager cannot guarantee multi-instance compatibility or enforcement safety. Roblox updates and anti-cheat changes can affect the feature without warning.
