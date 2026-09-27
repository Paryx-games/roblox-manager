# Manual Verification Checklist

Verify on Windows with Roblox installed. Start with account management and instance launching. Automated and synthetic UI checks have passed; the items below still need live verification.

## Account Refresh and Re-Add

- [ ] Refresh a valid account and confirm its name, avatar and presence update correctly.
- [ ] Re-add an existing account and confirm only one row remains.
- [ ] Confirm re-adding preserves its alias, group, pin and order.
- [ ] Scroll down a valid account, then switch to an account with expired credentials. Confirm recovery guidance appears at the top.

## Launching and Instances

- [ ] Launch two accounts into a game.
- [ ] Confirm running client counts and account attribution are correct.
- [ ] Confirm launches respect the configured spacing.
- [ ] Focus a running client and confirm the correct window becomes active.
- [ ] Arrange windows and confirm the clients use the configured layout.
- [ ] Join another account's server and confirm the selected account reaches the same server.
- [ ] Close an exact match and confirm only the intended client closes.
- [ ] Confirm individual kill controls are disabled for inferred and unmatched clients.

## Background Behaviour

- [ ] Navigate between pages while clients run and confirm counts keep updating.
- [ ] Confirm configured automatic window arrangement runs after launching.
- [ ] Confirm configured window naming works.
- [ ] Confirm original window titles return when naming is disabled or RM exits.
- [ ] Confirm kill-all honours the confirmation preference, including cancellation.
- [ ] Confirm configured privacy cleanup runs on exit.

## Assets

Use disposable files for upload checks. Uploading creates real assets and submits them to Roblox moderation.

- [ ] Upload a disposable asset under an account creator.
- [ ] Upload a disposable asset under a group the account can publish to.
- [ ] Edit a queued asset's name, type and creator and confirm the changes persist. Changing type should not convert the file's contents.
- [ ] Select only some queued rows and confirm the upload dialog lists those rows.
- [ ] Cancel upload confirmation and confirm nothing uploads.
- [ ] Confirm an accepted upload processes only the selected rows.
- [ ] Browse live creations and confirm thumbnails, type filters and pagination work.
- [ ] Grant an existing asset access to an experience and confirm the permission in Creator Dashboard.
- [ ] Test a manually entered experience ID and confirm management rights are checked.
- [ ] Reveal an imported file and confirm Explorer selects the correct file.
- [ ] Confirm copy actions produce the expected names, links and asset IDs.

## Startup and Storage

Use a separate test setup and backed-up test stores. Do not test recovery or start-over against your only copy of the account store.

- [ ] Unlock a legacy encrypted test store and confirm it upgrades successfully without losing accounts.
- [ ] Migrate older local test data and confirm original files remain available.
- [ ] Confirm favourites become preset files without duplicate entries on retry.
- [ ] Test the guarded start-over flow, including cancellation and the required confirmation text.
- [ ] Confirm start-over preserves encrypted recovery copies before replacing the active store.
- [ ] Confirm the first-launch tutorial supports navigation and skipping.
- [ ] Confirm the changelog appears after upgrading and does not keep appearing after acknowledgement.
- [ ] Confirm the one-time passwordless offer supports acceptance and decline without repeatedly prompting.
- [ ] Trigger a blocked browse-as play request and confirm the prompt prefills the Place ID without automatically launching.

## Interface

- [ ] Toggle name/avatar anonymisation and confirm managed account displays update appropriately.
- [ ] Toggle developer and utility visibility and confirm navigation and controls update.
- [ ] Confirm notifications stay compact across pages, including multiple notifications.
- [ ] Confirm notifications can be dismissed and do not obscure Settings' unsaved-changes controls.
- [ ] Confirm the file chooser and filename status appear on separate lines inside the same dashed box.
- [ ] Select a file and confirm its filename appears and its contents load correctly.
- [ ] Confirm switching accounts uses a small, smooth transition and resets detail scrolling.
- [ ] Confirm reduced-motion settings disable the account-switch animation.
- [ ] Export accounts to CSV and confirm the legacy columns, including `account_age` and `last_used`, contain the expected values.
