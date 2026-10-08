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

## Join another user's game

Open **App actions > Join user** in the title bar, or **Join game** from an account's
Connections search result. Enter an exact username or numeric user ID, choose
one or more accounts in **Launch with**, use **All accounts** to toggle every
available account, and choose:

- **Check selected accounts for a visible server:** RM checks target presence
  from each account in sequence. The first visible Place ID and Job ID is reused
  for launches from all chosen accounts. This can find friends-only presence
  when one selected account has access; Roblox still enforces access for each
  launching account, so learning the IDs does not guarantee admission.
- **Temporarily follow, join, then unfollow:** RM first checks whether the server
  is already visible. If needed, it checks the existing follow relationship,
  creates a temporary follow, checks presence again, requests the launch, and
  removes the temporary follow. Existing follows are kept. Following does not
  guarantee that Roblox will expose the server or permit a join.

Requests run in sequence using RM's existing CSRF handling and rate-limit
backoff. Terminal rate limiting stops further accounts; RM does not launch
concurrent requests to work around it. Launches also use your configured launch
delay. **Cancel remaining** stops future steps/accounts after the current
request and follow cleanup; an already submitted launch is not recalled.
**Launch requested** means Roblox was started, not that entry succeeded.

Temporary follow obligations are saved before following in encrypted
`join-follow-cleanup.dat` beside `config.json`, even with memory-only session
history. If Roblox rejects cleanup or RM closes during a request, reopen
**Join user** to review **Temporary follows need attention** and use **Retry
cleanup**. Review the relationship first: a request may have succeeded before
RM closed. Retry explicitly removes that follow. A missing/expired account must
be re-added/refreshed before retrying. Resetting the account store replaces its
key and cannot decrypt the old journal; review relationships on Roblox manually,
then use **Reset unreadable cleanup journal**. This forgets the unreadable record
and does not change any Roblox relationship.

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
