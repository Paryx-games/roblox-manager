# Changelog

## v1.13.0

### Added

- **Built-in MAC address rotation.** Privacy settings can rotate the active Windows network adapter's MAC address, preserving the PC's permanent adapter OUI by default or using a selectable well-known vendor OUI. Rotation is explicit, requires the NetAdapter PowerShell module, and may require administrator permissions.

## v1.12.4

### Added

- **Blocked browser launch fallback.** When Roblox cannot launch from the embedded browser, RM blocks the external player launch, brings its window to the foreground, flashes the taskbar, and offers to prepopulate the Place ID field.
- **Orphaned data cleanup.** Developer Options now includes a button to remove browse-as profiles for accounts that are no longer in RM.

### Changed

- **Clearer launch data fields.** Launch data now rejects `placeId`, `jobId`, and `gameInstanceId`, with guidance to use the dedicated Place ID and Job ID fields instead.
- **Centralized workspace version.** The root `Cargo.toml` now owns the application version, which both crates inherit and the update checker reports.

## v1.12.3

### Added

- **Launch data field.** Single and bulk Roblox launches accept optional data such as `?vip=true`, with examples shown below the field and support in saved presets.

### Changed

- **Readable info cards.** `infocards.json` now uses one multi-line object per card, making descriptions and card types easier to edit.
- **Cleaner settings spacing.** Reduced excess padding around the info icons while keeping their tooltips visually distinct.
- **Utility layout.** Utility is now an empty landing tab, while the optional Assets tab appears independently when enabled.
- **Colored warnings.** The Utility Hyperion notice now has a yellow background and border, and warning/caution info cards use matching yellow or red backgrounds and borders.
- **Reliable utility icons.** Utility and Tile Windows Now now use visible non-emoji icons so they render consistently across Windows font setups.
- **Update infocard texts.** Infocards have been updated for clarity and consistency

## v1.12.2

### Added

- **Debug startup diagnostics.** Debug builds and `cargo run` now keep a console open, log the app version and build profile, and show the RM data directory in startup logs.
- **Utility tab.** Utility can be enabled from Settings, with the Assets view independently controlled inside it.
- **Hyperion warning.** Utility shows a dismissible warning about Hyperion detection; it returns when RM is reopened.
- **RM data-folder shortcut.** Developer Options now includes a button to open the AppData folder where RM stores its files.

### Changed

- **Typed info cards.** Info-card entries now declare `info`, `warning`, or `caution` types. Their icons and hover panels use gray, yellow, or red styling respectively.
- **Info icon spacing.** Settings info icons now use a softer gray appearance with 10px padding.

## v1.12.1

### Changed

- **Info card icons softened.** Settings information icons are now slightly grayer and have a small amount of padding for a quieter visual footprint.

## v1.12.0

### Added

- **Settings explanations.** Relevant settings now show a hoverable information icon with descriptions loaded from `infocards.json`.
- **Theme-aware info icons.** Dark and light variants are embedded into the application executable.

### Changed

- **Roblox launches use the player executable directly.** RM now resolves and verifies `RobloxPlayerBeta.exe` for single and bulk launches instead of falling back to the `roblox-player:` protocol handler.
- **Missing Roblox player paths fail clearly.** RM refuses to launch when the configured or auto-detected executable is unavailable.

## v1.11.3

### Changed

- **Update checker moved to GitHub.** The app now checks the GitHub release feed for the current repo instead of the old GitLab project URL.
- **Top bar compacted for smaller UI widths.** Tab labels and status text shrink or truncate more gracefully when the window is narrow, keeping key controls usable without crowding the toolbar.

## v1.11.2

### Fixed

- **Auto-arrange timing.** Window tiling now waits for the full launch burst to settle before arranging Roblox clients, instead of firing on the first window to appear and re-laying out too early.
- **Settings persistence.** Settings changes now save to `config.json` immediately when the Settings tab is edited, so toggles and tiling values are no longer lost unless the user explicitly clicks Save after the fact.

## v1.11.1

### Changed

- **Account launch toggle disabled.** The account enable/disable launching feature was not working reliably and was causing confusion, so it has been disabled for now while the underlying issue is addressed.
- **Sidebar account categories removed.** The collapsible Enabled / Restricted / Disabled account sections were removed from the accounts list to keep the sidebar simpler and avoid the broken launch-state behavior.

## v1.11.0

### Added

- **Account launch controls.** Enable or disable launching for each account from its `...` menu. Disabled accounts remain available for management but cannot be launched by RM.
- **Launch-state account sections.** The sidebar now separates accounts into collapsible Enabled, Restricted by Roblox, and Disabled sections. Disabled rows are muted and marked clearly.
- **Import launch preference.** Add Account now offers an enabled-by-default checkbox that applies the chosen launch state to browser, manual, forced, and bulk imports.

### Changed

- Launch controls now block disabled and Roblox-restricted accounts across the main panel, quick launch, server join, bulk launch, and private-server launch paths.
- **Groups workspace.** Group discovery now uses the same find-and-results layout as the other management tabs, with search as the primary path and group-ID lookup as a secondary option.

## v1.10.0

### Added

- **Multi-monitor window tiling.** Roblox windows can now be tiled across any connected display. Pick a target — Primary Monitor, All Monitors (distributes evenly), or a specific monitor by name — from the new settings in the Launch Behavior section.
- **Custom grid layouts.** Six layout modes are available: Auto Grid (square-ish), Fixed Columns, Fixed Rows, Custom Grid (explicit Cols × Rows), Side-by-Side (one row), and Stacked (one column). Columns and rows are adjustable via drag inputs when applicable.
- **Window padding control.** A padding slider (0–50 px) adds a gap between tiled windows.
- **Tile Windows Now button.** Instantly rearrange all open Roblox windows at any time from Settings without needing to relaunch accounts.
- **Tile Windows from the group panel.** When multiple accounts are selected and Roblox is running, a Tile Windows button appears alongside Kill All Instances for quick access.
- **Account Age sorting.** Sort accounts by Roblox account age from youngest to oldest or oldest to youngest.
- **Sortable direction control.** Name and Account Age sorting now support ascending and descending directions. The direction control is disabled for Custom and Status sorting, where it does not apply.
- **Groups workspace.** Enter a Roblox group ID to view its icon, title, description, owner, verification state, member count, current announcement, and recent wall posts.
- **Group search and capability details.** Search public groups by name, select a result to load it, and view public-entry status, community tier, social-module availability, and creation date.
- **Selected-account group membership.** From the Groups tab, inspect each selected account's role and join or leave the loaded group for all selected accounts. Actions run through the account's authenticated session and report each result separately.

### Changed

- Sort mode and sort direction are persisted in settings and restored when RM starts.
- Pinned accounts remain above all other accounts regardless of the selected sort mode or direction.
- Group responses are parsed defensively because Roblox can return different payload shapes for announcements, posters, roles, and icons.

### Fixed

- **Groups tab response errors.** Group details, announcements, icons, membership checks, and join/leave actions no longer fail on common Roblox response-shape variations with a generic "error decoding response body" message.
- **Group membership 403s.** Membership requests now send the expected JSON body, skip accounts already in the requested state, and preserve Roblox's actual rejection reason instead of reporting every 403 as a rejected cookie.
- **Interactive group challenges.** When Roblox requires a security challenge that the API cannot complete automatically, the Groups page now explains the blocker and opens the community page for manual verification.
- **Authenticated group-page redirect.** The challenge recovery action now reuses the account browser window, logs in as the affected account, and redirects directly to that group's community page.
- **Wall feed availability.** Roblox's current public group API returns 404 for the legacy wall-post feed; the Groups page now uses the embedded group shout for announcements and explains when wall posts are unavailable instead of showing a misleading empty feed.

## v1.9.0

### Added

- **Pinned accounts.** Pin accounts from the sidebar so they stay at the top in every sort mode. The pin uses one icon whose color changes between muted and active states.
- **Per-account Roblox player paths.** Set a custom player executable for one account or a selection of accounts from the expanded `...` menu. Custom paths are stored in normal settings and override the global Settings path.
- **Account metadata.** The expanded account panel now shows the effective player path, Roblox account creation date, and account age in years, months, days, and hours.
- **Groups tab.** Added an empty Groups tab as the starting point for group management improvements.
- **Right-click an account to kill or focus its client.** Killing used to be all or nothing. RM re-checks the process is really that account's client before it terminates anything.
- **Join the server another account is in.** Right-click an account, pick one that is currently in a game, and it launches straight into that server.
- **Optional window naming.** Roblox windows can be named after their account so tiled clients are tellable apart. Off by default, in Settings. Unticking it puts the original titles back.

### Changed

- **Running clients are now matched to their account exactly.** RM reads a token off the client's own command line instead of guessing from the order windows appear in. Bulk launches no longer mix accounts up, and a Roblox you started yourself is no longer mistaken for one of RM's. Where the command line cannot be read, the old guess is still used and is labelled as one.
- Joining a specific server now uses the same request form the Roblox client itself sends.

### Fixed

- **Old log files are deleted on first launch.** Logs written before v1.8.1 could contain a Roblox authentication ticket. The current logs never do.

## v1.8.1

### Fixed

- **Your authentication ticket was being written to the log file.** Launching a game recorded the full launch URI at debug level, and that URI carries a live Roblox auth ticket. Logs are now scrubbed of cookies, auth tickets, CSRF tokens, and your Windows username.
- **Log files grew without limit.** `rm.log` is now a dated daily file and only the last 7 are kept.
- **Lower idle CPU.** The running instance counter walked the full process list on every frame.

### Added

- **Running Roblox clients are matched to the account that launched them.** Hover the instance counter to see which window belongs to which account. This is a best guess, so starting Roblox by hand or bulk launching quickly can attribute a window to the wrong account.

### Changed

- Color consistency fixes throughout the UI, where the same state was drawn in slightly different shades in different places.

## v1.8.0

### Added

- **No more master password.** New installs encrypt the account store with a key held in Windows Credential Manager, so RM opens straight to your accounts. The file on disk is still AES-256-GCM and is useless on its own.
- Existing password users are asked once, after unlocking, whether to switch. Declining is remembered.
- **Set a master password** is now a deliberate choice in Settings, along with **Stop asking for a password**. Both take effect immediately.

### Fixed

- **Changing your password could strand accounts.** Every cookie was re-encrypted one at a time and failures were skipped silently, leaving those accounts readable only with the old password. Re-keying now rewrites 32 bytes of header and touches no cookie at all.
- **Clearing the password did nothing on disk.** It only forgot the password in memory, left the file encrypted with it, and silently stopped saving.
- **Credential Manager mode never saved your account list.** Saving was gated on having a master password, which that mode never sets.
- **The old password still worked after changing it.** The backup copy was left encrypted under the retired password, and opening it rolled the store back.
- Master passwords now use Argon2id with a per-file salt, replacing unsalted SHA-256. The old format is read and upgraded automatically on first unlock.
- Cookies no longer run 100k hash rounds each time one is decrypted.

## v1.7.0

### Fixed

- **Preset chips did not wrap.** Past a certain number of presets, the last chip was squeezed into a one-letter-wide column at the edge of the panel instead of moving to the next row. Affected the launch panel and bulk launch.
- Preset names longer than 24 characters are shortened on the chip, with the full name on hover.

## v1.6.0

### Fixed

- **Granting universe access failed with "Invalid SubjectType is invalid".** The request nested the subject inside each asset entry. The endpoint takes one subject at the top level and a list of assets, and now gets it.
- **Partial grant failures were reported as successes.** A 200 from the grant endpoint can still refuse individual assets. Only what Roblox confirms is counted and recorded now, and refusals are named in the log.
- **"Nothing to grant" when granting access to assets picked from the inventory.** The library and the inventory share one selection but identify rows differently, and the grant only understood the library's. Inventory selections now work, signed by the account whose inventory is open.
- **Uploads no longer claim an asset passed moderation before it has.** A finished upload operation means Roblox ingested the file, not that the asset is usable. Rows now sit in a new **In review** state and only turn green once `develop.roblox.com` reports the review finished. Auto-grant waits for that too.
- **Bulk audio uploads failing in a block.** Audio now uploads one at a time with a gap between files, instead of three at once into Roblox's audio rate limit.
- **Uploads timing out on large files.** The 30 second request timeout covered the file transfer as well. Uploads get 5 minutes; everything else is unchanged.
- **Retryable failures are retried.** A rate limit or a transient 403 re-sends up to four times with a growing backoff. Only failures raised before the upload reached Roblox retry automatically, so nothing is uploaded twice.
- **Every failed row now has a Retry button.** It used to be hidden on failures the app judged permanent, which left a wrong guess with no way back.
- 429 backoff honours Roblox's `Retry-After`, and is jittered so concurrent uploads stop waking up together and colliding again.

## v1.5.0

### Added

- **Asset Manager tab**, behind a new **Developer options** toggle in Settings (off by default). Upload decals, audio, models, animations and video to Roblox from any saved account.
- **Bulk import.** Pick many files at once, or drop them anywhere on the window. Each row gets its own creator and asset type, and unsupported or oversized files are flagged in place rather than dropped.
- **Moderation tracking that survives restarts.** Operation IDs are saved to disk, so closing the app mid-upload does not lose the result. Uploads left in flight by a crash go back to the queue instead of being re-sent blind.
- **Bulk permission grants.** Select assets and give an experience permission to use them, picked from a dropdown of your experiences or by pasting a place or universe ID.
- **Auto-grant.** Set "Grant access to" on an import batch and each asset is granted as it clears moderation.
- **Library and inventory browsing.** A left tree with your uploads plus your own and your groups' inventories, and a sortable, searchable table.

### Changed

- The HTTP client can now send a pre-encoded body, so uploads reuse the existing CSRF rotation and rate-limit backoff instead of bypassing it.

## v1.4.6

### Fixed

- **Spammy "403 Forbidden after CSRF retry" notification.** A rate-limited request used up the retry meant for a rotated CSRF token. The two are now counted separately.
- **CSRF tokens are cached per account** instead of in one shared slot. Roblox ties a token to the session that asked for it, so every account switch ate a guaranteed 403.
- **Clearer error text.** A 403 with no CSRF challenge is a rejected cookie, not a CSRF failure, and now says so.
- **Thumbnails and presence stuck for every account** when a single cookie went bad. Thumbnails no longer send a cookie, and a failed avatar fetch no longer aborts the presence refresh.
- **Wrong avatars and game icons.** Roblox returns thumbnail results out of order and drops IDs it cannot resolve. Results are matched by ID now, not by position.
- **Background refresh timers** use wall-clock intervals instead of a frame counter, which drifted badly while the window sat idle.
- **Repeat error notifications** are suppressed for a minute, and the toast stack is capped at five.

## v1.4.5

### Fixed

- **Account corruption and false "wrong password" lockouts.** Two saves could overlap and tear the encrypted account file, which then failed to decrypt and was wrongly blamed on the master password. Saves are now atomic (temp file plus rename) and run one at a time, and a `.bak` copy is kept.
- **Automatic recovery.** If the account file fails to load, the `.bak` copy is tried and the main file is repaired from it. The error message no longer assumes a wrong password.
- Config and preset files now use the same atomic write, so a crash mid-save can't truncate them.

## v1.4.4

### Added

- **Bulk import** under Add Account: paste many cookies (newline, comma, semicolon, or tab separated) or browse a `.txt` / `.csv` file. Moderated accounts get added silently; failures are counted in a summary screen.
- **Launch delay** setting in seconds. Throttles single and bulk launches for users on Roblox-rate-limited IPs.
- **Blurred avatars in anonymize mode**, replacing the prior hide-entirely placeholder so accounts stay visually distinguishable.

### Fixed

- **Cookie input flicker** on long pastes. The Add Account field is now multi-line.
- **Empty-box Back button** in the Add Account dialog. The bundled font didn't ship the arrow glyph; the buttons now read plain "Back".
- **Preset chips with duplicate names** registered every click against the first chip. Each chip now gets a unique widget ID.

## v1.4.3

### Fixed

- **Spammy "Cookie expired" toast** — the notification only fires now when an account's cookie *just* went from valid to invalid, instead of every revalidation cycle (~5 minute interval) for every dead-cookie account.
- **Wrong wording for terminated accounts** — moderated/terminated accounts no longer get the "Cookie expired. Re-add with a fresh cookie." toast or banner. The moderation banner already covers it, and the "re-add" advice is incorrect when Roblox revoked the cookie as part of an enforcement action.

## v1.4.2

### Added

- **Open browser as account** — right-click an account (or use the new button on the launch panel) to open a webview signed in as that account. Useful for checking profiles, redeeming codes, or appealing moderation without juggling browser profiles.
- **Launch presets** — saved place + optional Job ID combos, persisted as individual JSON files under `%APPDATA%\RM\presets\` so you can hand-edit, share, or back them up. New "Presets" tab to create, edit, and delete them, with chip rows in both the single-launch and bulk-launch views. Existing favorites are migrated automatically on first launch.
- **Ban / moderation detection** — periodic revalidation now checks each account's moderation status via Roblox's public profile and `usermoderation.roblox.com` endpoints. Moderated accounts get an orange status dot in the sidebar, a banner in the account panel showing the specific reason and expiry, and a notification when moderation is first detected. Adding a moderated account prompts a confirmation with options to **Open browser as** (to investigate or appeal) or **Add anyway**.
- **Add anyway for rejected cookies** — if a cookie fails to validate (e.g. terminated alts), an inline "Add anyway" form lets you save the account by looking up the username via Roblox's public API. The cookie is stored as-is and marked expired until you resolve things in a browser.
- **Re-validate button** — on the moderation confirm dialog, resolve a warning in the browser then re-run validation without re-pasting your cookie.
- **Refresh all** button in the top bar — manually re-runs cookie validation, moderation checks, presence, and avatar refresh for every account.
- **Auto-add after browser login** — when the embedded login window captures your cookie, the account is added immediately instead of waiting for you to click "Add" again.

### Changed

- **UI overhaul** — Launch is now the visual hero of the account panel (large primary button row, accent color), labels float above inputs instead of right-aligned grids, and the Save-as-Preset form is collapsed into a single ⭐ button. The bottom status bar is gone; its info moved into the top bar. Remove Account moved into a `...` menu in the account header. Empty state has a friendlier illustration + heading.
- **Sidebar rows** — now show the cached avatar thumbnail with a presence dot overlaid on its bottom-right, plus the display name as a subtitle below the username.
- **Visible textboxes** — global style tweak adds a subtle border + rounding to every interactive widget so inputs no longer blend into their containers.
- **Shared Place ID / Job ID** — typing into single-account launch now populates the bulk-launch view too, and vice versa.
- **Account terminated banner** replaces the misleading "Cookie expired" message for accounts Roblox has revoked.
- **Cleaner Add Account modal** — dropped redundant headings, separators, and the `(N chars)` cookie-length annotation. The Back button is now a small chevron pinned top-left.
- **Em dashes removed** from all user-facing strings.

### Fixed

- **Tray Roblox kill** — periodic cleanup now uses a wall-clock timer instead of a frame counter, so it actually runs when the app is idle. Previously the check would only fire after the user generated 600+ UI events.
- **HTTP requests** — `Referer` and `x-bound-auth-token` headers are now sent on every request, matching real browser behavior. Fixes the moderation endpoint intermittently returning empty messages.
- **Moderation reason preservation** — periodic revalidation no longer overwrites a specific moderation reason with a generic placeholder when the moderation endpoint is temporarily unreachable.

## v1.4.1

### Fixed

- **First-launch tutorial** — step 3 now highlights the "Log in with browser" button and tells you to sign in with your Roblox account, instead of pointing at a cookie field that no longer exists on the first page of the Add Account dialog.

## v1.4.0

### Added

- **Log in with your Roblox account directly** — the Add Account dialog now has a "Log in with browser" option that opens a normal Roblox login window. Sign in as usual and RM will pick up your account automatically, with no need to copy cookies from your browser.

### Changed

- **Add Account dialog** — redesigned to ask how you'd like to add the account first (browser login or manual cookie paste), instead of showing both at once.
- **Cookie field** — when you do paste a cookie manually, the field is now a compact password-style input that hides the value, so the dialog stays small and your cookie isn't sitting on screen.
- **Master password prompt** — only appears when RM actually needs it. Once you've unlocked RM or set a master password, you won't be asked for it again when adding more accounts — and a mistyped password can no longer accidentally lock you out of the accounts you've already saved.

## v1.3.1

### Notice

- **Project moved to GitLab** — RM has moved from GitHub to GitLab. The new home is [gitlab.com/centerepic/robloxmanager](https://gitlab.com/centerepic/robloxmanager). Future releases and updates will be published there. The update checker has been switched to the new location.

## v1.3.0

### Added

- **Private server grouping** — private servers are now grouped by game with a thumbnail and game name in each group header.
- **Share link resolution** — paste an `rbxShareLink://` URL directly when adding a private server; RM resolves the access code automatically.
- **Game name and icon resolution** — game names and thumbnails are fetched in the background (no authentication required) and shown in the private servers tab.
- **Account groups** — accounts can be organised into named, colour-coded groups via drag-and-drop. Groups are collapsible and support bulk actions.
- **Custom account sorting** — accounts and groups can be reordered by dragging, or sorted alphabetically by name or by online status. Custom order is persisted across restarts.
- **Interactive first-launch tutorial** — new users see a 6-step guided walkthrough that highlights key UI elements (Add Account button, cookie field, account list, Launch button) and advances automatically as each action is completed.

### Fixed

- Private server name and icon were not resolving due to using an API endpoint that requires authentication. Switched to the unauthenticated `universeIds` endpoint.
- `universe_id` from the share link API response is now stored on the `PrivateServer` model and used for all subsequent name/icon lookups.
- UI no longer repaints continuously when idle; repaints are now triggered only when backend events arrive.

## v1.2.1

### Fixed

- **"What's New" window** — changelog now renders with proper formatting (headings, bold text, bullet points) instead of raw markdown.

## v1.2.0

### Added

- **Automatic update check** — on startup, checks GitLab for a newer release and shows a clickable "Update available" link in the top bar.
- **"What's New" changelog** — on the first launch after an update, a window displays the changelog for the new version.
- **Standard data directory** — config and account data now stored in `%APPDATA%\RM` instead of next to the exe, so the app works from any location.
- **Legacy data migration** — if existing data is found next to the exe, a native dialog offers to move it to the new location on startup.
- **Version in title bar** — the window title now shows the current version number.

## v1.1.0

### Added

- **Anonymize names** — new toggle in Settings > Privacy that replaces all usernames and display names with generic "Account 1", "Account 2", etc. throughout the UI.

### Fixed

- **Favorite places** — clicking a favorite button now correctly populates the Place ID field. Previously an invisible overlapping widget was stealing clicks.
- **Favorite deletion** — right-clicking a favorite now shows a proper context menu with a "Remove" option, replacing the non-functional previous approach.
- Favorites row now wraps when there are many entries instead of overflowing off-screen.

## v1.0.0

- Initial release.
