# New Features: Pinned Accounts & Custom Roblox Path

## Summary
Two new features have been added to the Roblox Manager:

### 1. Pinned Accounts
**What it does:** Pin specific accounts to always appear at the top of the sidebar, regardless of the sort order (Name, Status, or Custom).

**How to use:**
- Look for the **📌** icon on the right side of each account row in the sidebar
- Click the icon to toggle the pin state
- When unpinned, the same **📌** icon is gray; when pinned, it uses the accent color
- Hover over the icon for a tooltip ("Pin to top" / "Unpin account")

**Details:**
- Pinned accounts remain pinned across all three sort modes (Custom, Name, Status)
- Multiple accounts can be pinned simultaneously
- Within the pinned/unpinned groups, the normal sort order is maintained
- Pinned state is persisted in `%APPDATA%\RM\accounts.*` (encrypted account store)

### 2. Custom Roblox Player Path per Account
**What it does:** Allow specific accounts to launch with a different Roblox player installation, overriding the global player path in Settings.

**Field location:** `AppConfig` stores per-account paths in `custom_player_paths`, keyed by user ID.

**How to implement in UI (future enhancement):**
- You can add a context menu option: "Set custom Roblox path" for each account
- When an account has a custom path set, that path takes precedence over the global `config.roblox_player_path`
- The path is stored in the account data and persisted with the account store

**Implementation guide for connecting to UI:**
The field exists and is automatically saved/loaded. To add UI controls:
1. In `ram_ui/src/components/sidebar.rs`, add a right-click menu option
2. Emit a new `SidebarAction::SetCustomPath(user_id, path)` 
3. In `ram_ui/src/app.rs`, handle it by updating `self.config.custom_player_paths`
4. The path will be used during launch (this needs integration in the launch code path)

## Code Changes Made

### Model Layer (`ram_core/src/models.rs`)
- Added `is_pinned: bool` field to `Account` struct (defaults to `false`)
- Added `custom_player_paths: HashMap<u64, PathBuf>` to `AppConfig` (stored in normal `config.json`)
- Updated `Account::new()` to initialize both fields

### Sidebar Component (`ram_ui/src/components/sidebar.rs`)
- Added `TogglePinAccount(u64)` to `SidebarAction` enum
- Modified sorting logic in `show()` to prioritize pinned accounts:
  - In **Custom** mode: pinned first, then `sort_order`, then name
  - In **Name** mode: pinned first, then alphabetical
  - In **Status** mode: pinned first, then presence (online), then name
- Added pin button to `render_account_row()`:
  - Displays the same **📌** icon for every account
  - Uses muted gray when unpinned and the accent color when pinned
  - Button is positioned on the right side of the account row
  - Clicking toggles the pinned state
  - Hover text provides clear feedback

### App Layer (`ram_ui/src/app.rs`)
- Added handler for `SidebarAction::TogglePinAccount`:
  - Toggles the `is_pinned` flag on the account
  - Shows a toast notification ("Account pinned" / "Account unpinned")
  - Auto-saves the updated account store

## Data Persistence
- Pinned state is serialized to the encrypted account store
- Custom paths are serialized to normal `config.json`
- Data is automatically saved when accounts are modified via the UI
- All data remains encrypted in `%APPDATA%\RM\accounts.*`

## Testing
The implementation compiles successfully and maintains backward compatibility:
- Existing accounts without these fields use the defaults (`is_pinned = false`, no custom path)
- Serde's `#[serde(default)]` ensures older account files load correctly

## Optional: Custom Icon
Currently using the Unicode pin emoji (📌), recolored by state. If you'd like to use a custom PNG icon:
1. Place icon files in `assets/` folder (e.g., `assets/pin.png`, `assets/pin_active.png`)
2. Load them in the sidebar code using egui's `Image::from_bytes()` or file-based loading
3. Replace the emoji text rendering with the loaded images

---

**Ready to rebuild and test!** All changes are backward compatible and persist across restarts.
