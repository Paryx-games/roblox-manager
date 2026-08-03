# Changelog

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
