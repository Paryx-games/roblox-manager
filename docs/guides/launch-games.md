---
description: Start Roblox games and run multiple sessions.
icon: gamepad
---

# Launch games

Launch Roblox sessions from the account list. Choose the account first, then start the game you want.

{% stepper %}
{% step %}
### Select an account

Choose the Roblox account for this session.
{% endstep %}

{% step %}
### Choose a game

Select the Roblox game you want to launch.

Enter its numeric **Place ID**. Optionally provide a **Job ID** to target a particular server and supported launch **Data**. A Job ID is a server identifier, not another Place ID. Saved presets fill these fields for you; private-server links belong in **Private Servers**.
{% endstep %}

{% step %}
### Start the session

Launch the game and wait for the Roblox client to open.

An account with confirmed invalid credentials or active moderation cannot launch. Revalidate or replace its credential if appropriate. A successful launch request still depends on the installed Roblox client starting and the destination accepting the account.
{% endstep %}
{% endstepper %}

### Launch more than one session

Use multi-instance mode when you need multiple Roblox clients at once.

Select multiple accounts in Accounts with `Ctrl`-click or `Shift`-click, then use **Bulk launch** and review the destination. Successive launches use the configured pacing. Preset launches, private-server launches and server joins use the same launch safeguards.

### Browser links and background behaviour

When an account browser blocks an external Roblox play link, RM offers to prefill its Place ID in Accounts. It does not automatically launch the game; select an account and launch explicitly.

Closing the manager window can leave it running in the tray. Use the tray exit action to stop it. Closing the manager does not mean every Roblox client is closed; inspect them in [Instances](instances.md).

{% hint style="warning" %}
Multi-instance support depends on Roblox client behavior. Use it at your own risk.
{% endhint %}

<a href="multi-instance.md" class="button secondary" data-icon="clone">Learn about multi-instance</a>
