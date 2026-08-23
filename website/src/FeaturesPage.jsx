import { Link } from 'react-router-dom'

// transcribed directly from CHANGELOG.md - every Added/Changed entry that describes
// a user-facing feature. Fixed-only releases are skipped since those aren't "features".
const RELEASES = [
  {
    version: '1.12.3',
    items: [
      'Launch data field — single and bulk launches accept optional data like ?vip=true, with saved-preset support.',
      'Readable info cards — infocards.json now uses one multi-line object per card.',
      'Utility tab restructured — Utility is an empty landing tab; Assets appears independently when enabled.',
      'Colored warnings — Hyperion notice and warning/caution info cards use yellow or red backgrounds.',
      'Non-emoji utility icons that render consistently across Windows font setups.',
    ],
  },
  {
    version: '1.12.2',
    items: [
      'Debug startup diagnostics — debug builds log app version, build profile, and the RM data directory.',
      'Utility tab, enabled from Settings, with an independently controlled Assets view.',
      'Hyperion detection warning, dismissible, reappears on reopen.',
      'RM data-folder shortcut in Developer Options.',
      'Typed info cards — info, warning, or caution, each with matching icon and hover-panel styling.',
    ],
  },
  {
    version: '1.12.0',
    items: [
      'Settings explanations — hoverable info icons with descriptions from infocards.json, theme-aware.',
      'Roblox launches resolve RobloxPlayerBeta.exe directly instead of using the roblox-player: protocol handler.',
      'Launches fail clearly when the configured player executable is unavailable, instead of failing silently.',
    ],
  },
  {
    version: '1.11.3',
    items: [
      'Update checker moved to the GitHub release feed.',
      'Top bar compacts gracefully at smaller window widths.',
    ],
  },
  {
    version: '1.11.0',
    items: [
      'Account launch controls — enable or disable launching per account from its menu.',
      'Launch-state sidebar sections — Enabled, Restricted by Roblox, and Disabled.',
      'Import launch preference — set enabled-by-default state when importing accounts.',
      'Groups workspace redesigned around search-first discovery with group-ID lookup as a fallback.',
    ],
  },
  {
    version: '1.10.0',
    items: [
      'Multi-monitor window tiling across any connected display — Primary, All Monitors, or a specific monitor.',
      'Six tiling layouts — Auto Grid, Fixed Columns, Fixed Rows, Custom Grid, Side-by-Side, Stacked.',
      'Window padding control, 0–50px gap between tiled windows.',
      'Tile Windows Now button — rearrange open windows without relaunching.',
      'Tile Windows from the group panel when multiple accounts are selected.',
      'Account Age sorting, youngest to oldest or reverse.',
      'Ascending/descending direction control for Name and Account Age sorts.',
      'Groups workspace — icon, title, description, owner, verification, member count, announcement, wall posts.',
      'Group search by name with public-entry status, community tier, and creation date.',
      'Per-account group role inspection, plus bulk join/leave for selected accounts.',
    ],
  },
  {
    version: '1.9.0',
    items: [
      'Pinned accounts stay at the top of the sidebar in every sort mode.',
      'Per-account Roblox player paths, overriding the global Settings path.',
      'Account metadata panel — effective player path, Roblox creation date, and account age.',
      'Right-click an account to kill or focus its specific client, verified before terminating anything.',
      'Join the server another account is currently in, directly from the right-click menu.',
      'Optional window naming so tiled Roblox clients are tellable apart.',
      'Running clients matched to accounts by a token read off the client\'s own command line, not window order.',
    ],
  },
  {
    version: '1.8.1',
    items: [
      'Running clients matched to the account that launched them, shown on hover over the instance counter.',
    ],
  },
  {
    version: '1.8.0',
    items: [
      'No master password required — new installs encrypt the account store via Windows Credential Manager.',
      'Existing password users are offered a one-time switch to the new mode.',
      'Set a master password and Stop asking for a password added as explicit Settings choices.',
    ],
  },
  {
    version: '1.5.0',
    items: [
      'Asset Manager tab (behind a Developer options toggle) — upload decals, audio, models, animations, video.',
      'Bulk import via multi-select or drag-and-drop, each row independently typed and validated.',
      'Moderation tracking that survives restarts — operation IDs are saved to disk.',
      'Bulk permission grants — give an experience access to selected assets.',
      'Auto-grant — assets are granted to a chosen experience automatically as they clear moderation.',
      'Library and inventory browsing with a sortable, searchable table.',
    ],
  },
  {
    version: '1.4.2',
    items: [
      'Open browser as account — sign in as any saved account for profile checks, codes, or moderation appeals.',
      'Launch presets — saved place + Job ID combos, stored as individual JSON files you can hand-edit or share.',
      'Ban / moderation detection with reason, expiry, and a guided appeal flow.',
      'Add anyway for cookies that fail validation, resolved via Roblox\'s public username lookup.',
      'Re-validate button to recheck an account after resolving a warning in-browser.',
      'Refresh all button — re-runs validation, moderation checks, presence, and avatars for every account.',
      'Auto-add after browser login, without a second manual step.',
    ],
  },
  {
    version: '1.4.0',
    items: [
      'Log in with your Roblox account directly through an embedded browser window — no cookie copying required.',
    ],
  },
  {
    version: '1.3.0',
    items: [
      'Private servers grouped by game, with thumbnail and name per group.',
      'Share-link resolution — paste an rbxShareLink:// URL and RM resolves the access code automatically.',
      'Account groups — organise accounts into named, colour-coded, drag-and-drop groups with bulk actions.',
      'Custom account sorting — drag to reorder, or sort alphabetically or by online status.',
      'Interactive first-launch tutorial, six steps, advances automatically as each action completes.',
    ],
  },
  {
    version: '1.2.0',
    items: [
      'Automatic update check on startup with a clickable in-app notice.',
      '"What\'s New" changelog window shown after an update.',
      'Standard data directory (%APPDATA%\\RM) instead of storing data next to the executable.',
      'Legacy data migration offered automatically if old data is found.',
    ],
  },
  {
    version: '1.1.0',
    items: [
      'Anonymize names — replace every username and display name with generic placeholders throughout the UI.',
    ],
  },
  {
    version: '1.0.0',
    items: ['Initial release.'],
  },
]

export function FeaturesPage() {
  return (
    <main className="changelog-page">
      <div className="changelog-head">
        <p className="eyebrow">Every feature, every version</p>
        <h1>The full list.</h1>
        <p className="lead">
          Everything Roblox Manager can do, pulled straight from the changelog.
          Bug fixes are left out — this is just what it's grown to do since v1.0.0.
        </p>
        <Link className="secondary back-link" to="/">← Back home</Link>
      </div>

      <div className="changelog-list">
        {RELEASES.map((r) => (
          <section className="release" key={r.version}>
            <div className="release-version">
              <span className="version-tag">v{r.version}</span>
            </div>
            <ul className="release-items">
              {r.items.map((item) => (
                <li key={item}>{item}</li>
              ))}
            </ul>
          </section>
        ))}
      </div>
    </main>
  )
}