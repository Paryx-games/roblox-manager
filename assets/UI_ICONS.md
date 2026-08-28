# RM UI icon registry

Use these Windows-safe PNG assets in place of emoji glyphs. Each icon should
be a transparent square PNG with a simple monochrome mark, sized for 16 px
and 20 px UI use. Keep the adjacent text label visible for accessibility.

| Former glyph/use | Asset path | UI location |
| --- | --- | --- |
| accounts clipboard | `assets/icons/accounts.png` | top-bar Accounts tab and paste-cookie action |
| groups | `assets/icons/groups.png` | top-bar Groups tab |
| lock | `assets/icons/lock.png` | Servers tab and store unlock screens |
| star | `assets/icons/star.png` | Presets tab and preset controls |
| package | `assets/icons/package.png` | Assets tab |
| cap | `assets/icons/inventory.png` | Inventory tab |
| settings gear | `assets/icons/settings.png` | Settings tab |
| upload arrow | `assets/icons/update.png` | Update notification |
| globe | `assets/icons/browser.png` | Browser login and browser-as actions |
| download | `assets/icons/import.png` | Bulk import action |
| trash | `assets/icons/delete.png` | Remove and delete actions |
| skull | `assets/icons/kill.png` | Kill All action |
| disk | `assets/icons/save.png` | Save settings action |
| key | `assets/icons/password.png` | Password actions |
| folder | `assets/icons/folder.png` | Open containing folder action |
| pin | `assets/icons/pin.png` | Account pin control |
| window | `assets/icons/windows.png` | Tile Windows action |
| warning | `assets/icons/warning.png` | Warnings and error labels |

The current UI intentionally uses font-safe text labels until these assets
are present. When adding the PNGs, load them through egui's `Image::from_bytes`
pattern and keep the semantic label beside each icon rather than making the
image the only control affordance.
