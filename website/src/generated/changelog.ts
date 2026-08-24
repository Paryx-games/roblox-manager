export const releases = [
  {
    "version": "1.12.4",
    "items": [
      "Blocked browser launch fallback.. When Roblox cannot launch from the embedded browser, RM blocks the external player launch, brings its window to the foreground, flashes the taskbar, and offers to prepopulate the Place ID field.",
      "Orphaned data cleanup.. Developer Options now includes a button to remove browse-as profiles for accounts that are no longer in RM.",
      "Clearer launch data fields.. Launch data now rejects `placeId`, `jobId`, and `gameInstanceId`, with guidance to use the dedicated Place ID and Job ID fields instead.",
      "Centralized workspace version.. The root `Cargo.toml` now owns the application version, which both crates inherit and the update checker reports."
    ]
  },
  {
    "version": "1.12.3",
    "items": [
      "Launch data field.. Single and bulk Roblox launches accept optional data such as `?vip=true`, with examples shown below the field and support in saved presets.",
      "Readable info cards.. `infocards.json` now uses one multi-line object per card, making descriptions and card types easier to edit.",
      "Cleaner settings spacing.. Reduced excess padding around the info icons while keeping their tooltips visually distinct.",
      "Utility layout.. Utility is now an empty landing tab, while the optional Assets tab appears independently when enabled.",
      "Colored warnings.. The Utility Hyperion notice now has a yellow background and border, and warning/caution info cards use matching yellow or red backgrounds and borders.",
      "Reliable utility icons.. Utility and Tile Windows Now now use visible non-emoji icons so they render consistently across Windows font setups.",
      "Update infocard texts.. Infocards have been updated for clarity and consistency"
    ]
  },
  {
    "version": "1.12.2",
    "items": [
      "Debug startup diagnostics.. Debug builds and `cargo run` now keep a console open, log the app version and build profile, and show the RM data directory in startup logs.",
      "Utility tab.. Utility can be enabled from Settings, with the Assets view independently controlled inside it.",
      "Hyperion warning.. Utility shows a dismissible warning about Hyperion detection; it returns when RM is reopened.",
      "RM data-folder shortcut.. Developer Options now includes a button to open the AppData folder where RM stores its files.",
      "Typed info cards.. Info-card entries now declare `info`, `warning`, or `caution` types. Their icons and hover panels use gray, yellow, or red styling respectively.",
      "Info icon spacing.. Settings info icons now use a softer gray appearance with 10px padding."
    ]
  },
  {
    "version": "1.12.1",
    "items": [
      "Info card icons softened.. Settings information icons are now slightly grayer and have a small amount of padding for a quieter visual footprint."
    ]
  },
  {
    "version": "1.12.0",
    "items": [
      "Settings explanations.. Relevant settings now show a hoverable information icon with descriptions loaded from `infocards.json`.",
      "Theme-aware info icons.. Dark and light variants are embedded into the application executable.",
      "Roblox launches use the player executable directly.. RM now resolves and verifies `RobloxPlayerBeta.exe` for single and bulk launches instead of falling back to the `roblox-player:` protocol handler.",
      "Missing Roblox player paths fail clearly.. RM refuses to launch when the configured or auto-detected executable is unavailable."
    ]
  },
  {
    "version": "1.11.3",
    "items": [
      "Update checker moved to GitHub.. The app now checks the GitHub release feed for the current repo instead of the old GitLab project URL.",
      "Top bar compacted for smaller UI widths.. Tab labels and status text shrink or truncate more gracefully when the window is narrow, keeping key controls usable without crowding the toolbar."
    ]
  },
  {
    "version": "1.11.1",
    "items": [
      "Account launch toggle disabled.. The account enable/disable launching feature was not working reliably and was causing confusion, so it has been disabled for now while the underlying issue is addressed.",
      "Sidebar account categories removed.. The collapsible Enabled / Restricted / Disabled account sections were removed from the accounts list to keep the sidebar simpler and avoid the broken launch-state behavior."
    ]
  },
  {
    "version": "1.11.0",
    "items": [
      "Account launch controls.. Enable or disable launching for each account from its `...` menu. Disabled accounts remain available for management but cannot be launched by RM.",
      "Launch-state account sections.. The sidebar now separates accounts into collapsible Enabled, Restricted by Roblox, and Disabled sections. Disabled rows are muted and marked clearly.",
      "Import launch preference.. Add Account now offers an enabled-by-default checkbox that applies the chosen launch state to browser, manual, forced, and bulk imports.",
      "Launch controls now block disabled and Roblox-restricted accounts across the main panel, quick launch, server join, bulk launch, and private-server launch paths.",
      "Groups workspace.. Group discovery now uses the same find-and-results layout as the other management tabs, with search as the primary path and group-ID lookup as a secondary option."
    ]
  },
  {
    "version": "1.10.0",
    "items": [
      "Multi-monitor window tiling.. Roblox windows can now be tiled across any connected display. Pick a target — Primary Monitor, All Monitors (distributes evenly), or a specific monitor by name — from the new settings in the Launch Behavior section.",
      "Custom grid layouts.. Six layout modes are available: Auto Grid (square-ish), Fixed Columns, Fixed Rows, Custom Grid (explicit Cols × Rows), Side-by-Side (one row), and Stacked (one column). Columns and rows are adjustable via drag inputs when applicable.",
      "Window padding control.. A padding slider (0–50 px) adds a gap between tiled windows.",
      "Tile Windows Now button.. Instantly rearrange all open Roblox windows at any time from Settings without needing to relaunch accounts.",
      "Tile Windows from the group panel.. When multiple accounts are selected and Roblox is running, a Tile Windows button appears alongside Kill All Instances for quick access.",
      "Account Age sorting.. Sort accounts by Roblox account age from youngest to oldest or oldest to youngest.",
      "Sortable direction control.. Name and Account Age sorting now support ascending and descending directions. The direction control is disabled for Custom and Status sorting, where it does not apply.",
      "Groups workspace.. Enter a Roblox group ID to view its icon, title, description, owner, verification state, member count, current announcement, and recent wall posts.",
      "Group search and capability details.. Search public groups by name, select a result to load it, and view public-entry status, community tier, social-module availability, and creation date.",
      "Selected-account group membership.. From the Groups tab, inspect each selected account's role and join or leave the loaded group for all selected accounts. Actions run through the account's authenticated session and report each result separately.",
      "Sort mode and sort direction are persisted in settings and restored when RM starts.",
      "Pinned accounts remain above all other accounts regardless of the selected sort mode or direction.",
      "Group responses are parsed defensively because Roblox can return different payload shapes for announcements, posters, roles, and icons."
    ]
  },
  {
    "version": "1.9.0",
    "items": [
      "Pinned accounts.. Pin accounts from the sidebar so they stay at the top in every sort mode. The pin uses one icon whose color changes between muted and active states.",
      "Per-account Roblox player paths.. Set a custom player executable for one account or a selection of accounts from the expanded `...` menu. Custom paths are stored in normal settings and override the global Settings path.",
      "Account metadata.. The expanded account panel now shows the effective player path, Roblox account creation date, and account age in years, months, days, and hours.",
      "Groups tab.. Added an empty Groups tab as the starting point for group management improvements.",
      "Right-click an account to kill or focus its client.. Killing used to be all or nothing. RM re-checks the process is really that account's client before it terminates anything.",
      "Join the server another account is in.. Right-click an account, pick one that is currently in a game, and it launches straight into that server.",
      "Optional window naming.. Roblox windows can be named after their account so tiled clients are tellable apart. Off by default, in Settings. Unticking it puts the original titles back.",
      "Running clients are now matched to their account exactly.. RM reads a token off the client's own command line instead of guessing from the order windows appear in. Bulk launches no longer mix accounts up, and a Roblox you started yourself is no longer mistaken for one of RM's. Where the command line cannot be read, the old guess is still used and is labelled as one.",
      "Joining a specific server now uses the same request form the Roblox client itself sends."
    ]
  },
  {
    "version": "1.8.1",
    "items": [
      "Running Roblox clients are matched to the account that launched them.. Hover the instance counter to see which window belongs to which account. This is a best guess, so starting Roblox by hand or bulk launching quickly can attribute a window to the wrong account.",
      "Color consistency fixes throughout the UI, where the same state was drawn in slightly different shades in different places."
    ]
  },
  {
    "version": "1.8.0",
    "items": [
      "No more master password.. New installs encrypt the account store with a key held in Windows Credential Manager, so RM opens straight to your accounts. The file on disk is still AES-256-GCM and is useless on its own.",
      "Existing password users are asked once, after unlocking, whether to switch. Declining is remembered.",
      "Set a master password. is now a deliberate choice in Settings, along with Stop asking for a password. Both take effect immediately."
    ]
  },
  {
    "version": "1.5.0",
    "items": [
      "Asset Manager tab. , behind a new Developer options toggle in Settings (off by default). Upload decals, audio, models, animations and video to Roblox from any saved account.",
      "Bulk import.. Pick many files at once, or drop them anywhere on the window. Each row gets its own creator and asset type, and unsupported or oversized files are flagged in place rather than dropped.",
      "Moderation tracking that survives restarts.. Operation IDs are saved to disk, so closing the app mid-upload does not lose the result. Uploads left in flight by a crash go back to the queue instead of being re-sent blind.",
      "Bulk permission grants.. Select assets and give an experience permission to use them, picked from a dropdown of your experiences or by pasting a place or universe ID.",
      "Auto-grant.. Set \"Grant access to\" on an import batch and each asset is granted as it clears moderation.",
      "Library and inventory browsing.. A left tree with your uploads plus your own and your groups' inventories, and a sortable, searchable table.",
      "The HTTP client can now send a pre-encoded body, so uploads reuse the existing CSRF rotation and rate-limit backoff instead of bypassing it."
    ]
  },
  {
    "version": "1.4.4",
    "items": [
      "Bulk import. under Add Account: paste many cookies (newline, comma, semicolon, or tab separated) or browse a `.txt` / `.csv` file. Moderated accounts get added silently; failures are counted in a summary screen.",
      "Launch delay. setting in seconds. Throttles single and bulk launches for users on Roblox-rate-limited IPs.",
      "Blurred avatars in anonymize mode. , replacing the prior hide-entirely placeholder so accounts stay visually distinguishable."
    ]
  },
  {
    "version": "1.4.2",
    "items": [
      "Open browser as account. — right-click an account (or use the new button on the launch panel) to open a webview signed in as that account. Useful for checking profiles, redeeming codes, or appealing moderation without juggling browser profiles.",
      "Launch presets. — saved place + optional Job ID combos, persisted as individual JSON files under `%APPDATA%\\RM\\presets\\` so you can hand-edit, share, or back them up. New \"Presets\" tab to create, edit, and delete them, with chip rows in both the single-launch and bulk-launch views. Existing favorites are migrated automatically on first launch.",
      "Ban / moderation detection. — periodic revalidation now checks each account's moderation status via Roblox's public profile and `usermoderation.roblox.com` endpoints. Moderated accounts get an orange status dot in the sidebar, a banner in the account panel showing the specific reason and expiry, and a notification when moderation is first detected. Adding a moderated account prompts a confirmation with options to Open browser as (to investigate or appeal) or Add anyway.",
      "Add anyway for rejected cookies. — if a cookie fails to validate (e.g. terminated alts), an inline \"Add anyway\" form lets you save the account by looking up the username via Roblox's public API. The cookie is stored as-is and marked expired until you resolve things in a browser.",
      "Re-validate button. — on the moderation confirm dialog, resolve a warning in the browser then re-run validation without re-pasting your cookie.",
      "Refresh all. button in the top bar — manually re-runs cookie validation, moderation checks, presence, and avatar refresh for every account.",
      "Auto-add after browser login. — when the embedded login window captures your cookie, the account is added immediately instead of waiting for you to click \"Add\" again.",
      "UI overhaul. — Launch is now the visual hero of the account panel (large primary button row, accent color), labels float above inputs instead of right-aligned grids, and the Save-as-Preset form is collapsed into a single ⭐ button. The bottom status bar is gone; its info moved into the top bar. Remove Account moved into a `...` menu in the account header. Empty state has a friendlier illustration + heading.",
      "Sidebar rows. — now show the cached avatar thumbnail with a presence dot overlaid on its bottom-right, plus the display name as a subtitle below the username.",
      "Visible textboxes. — global style tweak adds a subtle border + rounding to every interactive widget so inputs no longer blend into their containers.",
      "Shared Place ID / Job ID. — typing into single-account launch now populates the bulk-launch view too, and vice versa.",
      "Account terminated banner. replaces the misleading \"Cookie expired\" message for accounts Roblox has revoked.",
      "Cleaner Add Account modal. — dropped redundant headings, separators, and the `(N chars)` cookie-length annotation. The Back button is now a small chevron pinned top-left.",
      "Em dashes removed. from all user-facing strings."
    ]
  },
  {
    "version": "1.4.0",
    "items": [
      "Log in with your Roblox account directly. — the Add Account dialog now has a \"Log in with browser\" option that opens a normal Roblox login window. Sign in as usual and RM will pick up your account automatically, with no need to copy cookies from your browser.",
      "Add Account dialog. — redesigned to ask how you'd like to add the account first (browser login or manual cookie paste), instead of showing both at once.",
      "Cookie field. — when you do paste a cookie manually, the field is now a compact password-style input that hides the value, so the dialog stays small and your cookie isn't sitting on screen.",
      "Master password prompt. — only appears when RM actually needs it. Once you've unlocked RM or set a master password, you won't be asked for it again when adding more accounts — and a mistyped password can no longer accidentally lock you out of the accounts you've already saved."
    ]
  },
  {
    "version": "1.3.0",
    "items": [
      "Private server grouping. — private servers are now grouped by game with a thumbnail and game name in each group header.",
      "Share link resolution. — paste an `rbxShareLink://` URL directly when adding a private server; RM resolves the access code automatically.",
      "Game name and icon resolution. — game names and thumbnails are fetched in the background (no authentication required) and shown in the private servers tab.",
      "Account groups. — accounts can be organised into named, colour-coded groups via drag-and-drop. Groups are collapsible and support bulk actions.",
      "Custom account sorting. — accounts and groups can be reordered by dragging, or sorted alphabetically by name or by online status. Custom order is persisted across restarts.",
      "Interactive first-launch tutorial. — new users see a 6-step guided walkthrough that highlights key UI elements (Add Account button, cookie field, account list, Launch button) and advances automatically as each action is completed."
    ]
  },
  {
    "version": "1.2.0",
    "items": [
      "Automatic update check. — on startup, checks GitLab for a newer release and shows a clickable \"Update available\" link in the top bar.",
      "\"What's New\" changelog. — on the first launch after an update, a window displays the changelog for the new version.",
      "Standard data directory. — config and account data now stored in `%APPDATA%\\RM` instead of next to the exe, so the app works from any location.",
      "Legacy data migration. — if existing data is found next to the exe, a native dialog offers to move it to the new location on startup.",
      "Version in title bar. — the window title now shows the current version number."
    ]
  },
  {
    "version": "1.1.0",
    "items": [
      "Anonymize names. — new toggle in Settings > Privacy that replaces all usernames and display names with generic \"Account 1\", \"Account 2\", etc. throughout the UI."
    ]
  }
] as const
