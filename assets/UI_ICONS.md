# RM UI icon registry

Use these Windows-safe Lucide SVG assets in place of emoji glyphs. Each icon
should be a transparent square SVG with a simple monochrome mark and a
`viewBox="0 0 24 24"`. Keep the adjacent text label visible for accessibility.

| Former glyph/use | Asset path | UI location |
| --- | --- | --- |
| accounts clipboard | `assets/icons/accounts.svg` | Accounts navigation and paste-cookie action |
| groups | `assets/icons/groups.svg` | Groups navigation |
| lock | `assets/icons/lock.svg` | Servers tab and store unlock screens |
| star | `assets/icons/star.svg` | Presets tab and preset controls |
| package | `assets/icons/package.svg` | Assets tab |
| cap | `assets/icons/inventory.svg` | Inventory tab |
| settings gear | `assets/icons/settings.svg` | Settings tab |
| upload arrow | `assets/icons/update.svg` | Update notification |
| globe | `assets/icons/browser.svg` | Browser login and browser-as actions |
| download | `assets/icons/import.svg` | Bulk import action |
| trash | `assets/icons/delete.svg` | Remove and delete actions |
| skull | `assets/icons/kill.svg` | Kill All action |
| disk | `assets/icons/save.svg` | Save settings action |
| key | `assets/icons/password.svg` | Password actions |
| folder | `assets/icons/folder.svg` | Open containing folder action |
| pin | `assets/icons/pin.svg` | Account pin control |
| window | `assets/icons/windows.svg` | Tile Windows action |
| warning | `assets/icons/warning.svg` | Warnings and error labels |

## Active Tauri frontend

React loads the icon set from `ram_ui/frontend/public/icons/` using shared icon controls. Keep the set consistent with `assets/icons/`; use semantic names, accessible button labels and no emoji substitutes. See `DESIGN.md` for the active design rules.

Application, installer and uninstaller icons are separate Windows ICO files selected by `ram_ui/scripts/tauri.mjs`: Development for debug/dev, Alpha or Beta for those pre-release versions, and Live for stable or release candidates. The installer sidebar is the 164 x 314 bitmap under `ram_ui/src-tauri/installer/`, generated from `assets/branding/LogoThumbVertical.png`.
