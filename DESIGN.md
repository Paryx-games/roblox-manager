# RM interface guide

This document describes the active Tauri + React interface and sets the shared
visual conventions for future changes. It is based on the current v2 source.
The current interface is dark mode only. This guide distinguishes implemented
behavior from requirements for new changes; existing exceptions are not a
license to introduce new ones. AGENTS.md remains the repository policy.

## Source files

- `ram_ui/frontend/` contains the React pages and shared components.
- `ram_ui/frontend/styles.css` imports Tailwind CSS 4, imports the shared
  token file, defines the Tailwind theme mapping, and contains the app's page
  and component styles.
- `ram_ui/frontend/tokens.css` defines the shared color, type, spacing, radius,
  motion, and layout values. The active frontend imports it from `styles.css`.
- `ram_ui/.stylelintrc.json` and the package scripts define automated
  frontend linting.
- `ram_ui/frontend/components/Walkthrough.css` styles the guided first-run tour.
- `ram_ui/src-tauri/tauri.conf.json` defines the native window dimensions.

Most of the interface uses named CSS classes. Tailwind utilities are used in
the app shell and for a small number of mapped theme values. Do not assume
that every token has a matching Tailwind utility.

## Window and navigation

The main Tauri window opens at 1200 x 760 and cannot be resized below that
size. It currently has a 36px title bar, a 56px icon navigation rail, and a flexible
main content area. The settings page and some workspaces have their own
internal sidebars. Their widths belong to those pages, not the app shell.

The title bar shows running/pending counts, the version, an available-update
action, and custom window controls. The navigation rail uses 44px buttons,
local icons, tooltips, and a blue active indicator. Accounts, Instances,
Groups, Private Servers, and Presets are the regular workspaces. Inventories
and Asset Manager appear when developer options are enabled; Settings sits
below the rail spacer.

`--titlebar-height` still declares 40px but does not size the current title
bar. AGENTS.md also describes 40px. The implemented 36px value is recorded
here rather than changing the UI or silently treating the stale token as
the rendered height. Check the stylesheet and walkthrough inset together
before any future shell sizing change.

## Workspace layout

Keep the compact desktop layout and each page's existing composition:

- Accounts combines a compact header, account selection and actions, and
  account details and recovery flows.
- Instances presents running clients in a table with attribution and process actions.
- Groups uses account/group selection and detail sections for group content.
- Private Servers and Presets use their existing lists, filters, and editing flows.
- Inventories uses a 220px account sidebar, a grid/list workspace, and a
  resizable, collapsible filter panel on the right.
- Asset Manager uses creation/upload controls and tabular results.
- Settings uses a 200px internal sidebar and section navigation.

Use `min-width: 0` and `min-height: 0` where flexible panes need to shrink.
Preserve pane scrolling and overlay placement instead of making the whole
desktop shell scroll. Page headers commonly use `.header-row` (42px) and
`.header-title` (14px bold); larger headings use the type tokens appropriate
to their existing page. Do not force all pages into a new card or header pattern.

The frontend stylesheet includes media rules for narrower content, but a
window below the configured minimum is not a supported main-window size.
Window dimensions are CSS logical pixels; Windows and WebView2 apply display
scaling.

## Shared visual tokens

Use `var(--token-name)` for shared visual values. The current token values
are listed in `ram_ui/frontend/tokens.css`; update that file when a shared value
changes.

| Purpose | Tokens |
| --- | --- |
| Surfaces | `--bg-canvas`, `--bg-surface`, `--bg-raised` |
| Text | `--text-primary`, `--text-muted`, `--text-disabled` |
| Borders and actions | `--border-default`, `--action-primary`, `--action-muted` |
| Live status | `--status-online`, `--status-warning`, `--status-danger`, `--status-neutral` |
| Other semantic states | `--notification-*`, `--access-*`, `--ownership-*` |
| Guided tour | `--walkthrough-outline`, `--walkthrough-dim`, `--walkthrough-dock` |
| Typography | `--font-sans`, `--text-xs`, `--text-sm`, `--text-base`, `--text-lg`, `--text-xl` |
| Spacing | `--space-1`, `--space-2`, `--space-3`, `--space-4`, `--space-6`, `--space-8`, `--space-12` |
| Shape and motion | `--radius`, `--border-width`, `--duration-*`, `--ease-standard` |

Use Roboto for interface text. The icon component loads the project's local
icon set; use it for interface actions instead of emoji or one-off icon
libraries. Keep live account and process status distinct from static access
permissions, notification severity, and private-server ownership.

### Color, shape, and typography

Use the three neutral surface levels for depth, with no box shadows. Blue
`--action-primary` identifies actions, selection, and keyboard focus.
Reserve live-status colors for account, instance, and process state; use
notification, access, and ownership tokens for their separate meanings.
Do not rely on color alone: retain text, icons, or accessible labels.

The current stylesheet has legacy exceptions: close-window hover and some
destructive controls use `--status-danger`, warning UI uses `--status-warning`, and
`--notification-warning` aliases that token. These existing uses do not
change the semantic rule for new controls.

The shared radius is 6px and the border weight is 1px. Circular status dots,
round indicators, and partial-radius edge markers are existing geometric
exceptions, not alternate card/input radius styles. Focus outlines are
typically 2px with a 2px offset and are distinct from surface borders.

Roboto is bundled locally at weights 400, 500, and 700. The type scale is
11, 13, 14, 16, and 20px. Use the shared 4px spacing scale for padding,
margins, and gaps. Fixed shell regions, icon dimensions, pane widths, and
focus offsets are structural dimensions, not additional spacing tokens.
Existing compact layouts also use calculated token values; reuse the
relevant pattern instead of inventing a new scale.

### Icons

Use the project's PNG/SVG assets and `components/Icon.tsx`. Icons normally
load from `/icons/<name>.svg`; the component also supplies current-color SVG
variants for supported semantic actions and notifications. The shell has
its own local image helper. Keep decorative icons hidden from assistive
technology and name their containing controls. Do not add emoji or another
icon library.

## Existing shared components

Reuse a component when it already covers the needed behavior:

- `Icon`, `AccountAvatar`, and `AccountPicker` for common account and icon UI.
- `Select` for the custom keyboard-operable selection menu.
- `Popup`, `PopupMenu`, and `PromptModal` for dialogs, action menus, and text prompts.
- `TooltipProvider` for shared tooltips.
- `LoadingSkeleton` for pending page and data states.
- `Toast` for operation feedback.
- `ConfirmModal` for confirmation flows, alongside `PromptModal` for text input.
- `ReleaseNotes`, `Walkthrough`, and `WalkthroughAccounts` for startup guidance.

`Toast` and `ConfirmModal` currently live at the frontend root rather than
inside `components/`. Reuse them in place; this guide does not require a move.

Page headers, tables, cards, empty states, and forms are currently composed
from page markup and shared CSS. There is no shared `PageHeader`,
`DataTable`, `Card`, or dev-only design-system gallery. Add a shared
component when it removes real duplication or provides behavior that pages
would otherwise implement inconsistently.

## Interaction and motion

Interactive controls should expose a visible keyboard focus state, a clear
disabled state, and pending feedback for operations that take time. Preserve
the page-specific loading, empty, error, and retry behavior around data
requests. Icon-only buttons need accessible names.

### Interaction states

| State | Required behavior for applicable controls |
| --- | --- |
| Default | A readable label or accessible name and a clear action target |
| Hover | The existing surface/text treatment for the control family |
| Focus | Visible keyboard focus, including inside scrollable panes |
| Active/selected | The existing pressed or selected treatment, with semantic state where applicable |
| Disabled | Native disabled behavior where possible, subdued appearance, and no actionable hover treatment |
| Pending | Visible progress or a pending label; prevent duplicate submissions |
| Error | Visible, safe error text and a recovery/retry action when available |
| Empty | Explain the absence of data and show the relevant next action |

`Popup` supplies dialog focus containment and restores previous focus when
closed. Escape dismissal is available when an `onClose` callback is passed;
backdrop dismissal is opt-in. Give dialogs accessible titles and keep
pending/destructive flow dismissal consistent with the existing caller.
`PopupMenu` supplies initial focus, arrow/Home/End navigation, Escape dismissal
and focus restoration, and Tab dismissal. Callers provide the close callback
and own positioning and outside-pointer dismissal. Escape dismisses only the
foreground overlay; a nested menu closes before its containing dialog.

`Select` keeps focus on its trigger and supports arrow navigation,
Enter/Space selection, Escape, and disabled options. `TooltipProvider`
provides shared `data-tip` tooltips. Tooltips supplement accessible names.

Loading skeletons use page-specific layouts with a status announcement and
`aria-busy`. Toasts distinguish info, success, warning, and error; errors
use `role="alert"`, other kinds use `role="status"`. Their current visible
durations are 3, 5, or 8 seconds, with a dismiss action and exit animation.
The countdown pauses while a notification is hovered or contains keyboard focus.
Keep persistent failures visible in the affected flow when a transient
notification would not be enough.

Settings prompts to save, discard, or stay before navigation would abandon
unsaved preferences. Failed saves keep the draft available for correction.

Most shell text is not selectable. Inputs, identifiers, content, and error
messages have explicit selectable-text rules. Preserve useful copying and
the page's selection shortcuts without interfering with text editing.

### Motion

Use the shared 100/160/240ms durations and standard easing. Loading
skeletons use the 1200ms loading duration. Page changes display their destination
immediately and slide/fade in through the page-transition layer over 240ms. Reduced-motion CSS removes the
animation; navigation has no animation-related switching delay.

The app responds to `prefers-reduced-motion: reduce`. Keep transitions tied
to the user's action, and ensure page changes and loading indicators remain
understandable when motion is reduced. Menus and dialogs should support their
existing keyboard dismissal and focus behavior.

The first-run walkthrough uses dedicated dimming, outline, and dock tokens,
highlights shell/page targets, and renders demo accounts. Its translucent,
blurred dock is an existing tour-specific treatment, not a general panel
style. Keep the tour aligned with navigation and scrolling changes.

## CSS and lint

Run `pnpm --dir ram_ui lint` after CSS or component changes. ESLint checks
the TypeScript source. Stylelint excludes `tokens.css`, where token colors
are defined, and rejects hex colors and `!important` in component and page
CSS. It applies the standard CSS rules configured in
`ram_ui/.stylelintrc.json`. The linter does not verify the full design
system, keyboard behavior, contrast, or responsive appearance; review those
in the affected interface.

Use token colors, Roboto, the shared radius, and existing focus treatments.
Avoid shadows and new one-off visual patterns. Keep CSS near the current
page styles unless multiple pages share the same visual behavior.

Frontend operations go through typed wrappers in `lib/ipc.ts`. Preserve
visible pending/error states and clean up event listeners and timers.
Never expose credentials in frontend state, notification text, or error
details merely to simplify a UI flow.

## UI review checklist

When a change affects the interface, review the affected flow at the
supported window size and check:

- Keyboard navigation, focus visibility, accessible names, and Escape behavior.
- Loading, empty, error, retry, disabled, and saving states that apply.
- Menus, popups, and tooltips near scroll boundaries and window edges.
- Readability at the minimum supported window size and the target Windows
  display scaling.
- Reduced-motion behavior, Roboto usage, token colors, and the local icon set.

Record manual checks in the pull request. Lint and type checking do not
replace visual or keyboard review.

This document was checked against the current source. It does not certify
that every existing page satisfies every accessibility or interaction
requirement. For new visual patterns not covered here, discuss the approach
before implementation as required by AGENTS.md.

## Session workspace navigation

View preferences and selected identifiers are retained in memory across workspace
changes. Credential inputs, passwords, webhook URLs, private-server links,
and launch data are excluded. Panes restore their scroll positions after data
loads; user input cancels pending scroll restoration. Nothing is written to disk.
Settings search lives in the existing 200px section sidebar and jumps/focuses a
matching section without filtering away controls. The running-client count opens
Instances; identified instance accounts link back to their account details.

The Operation centre uses the shared Popup and table styles. It shows the latest
100 fixed-label summaries and live launch/upload stages for this session, supports
status filters, and links back to the owning workspace. Clearing completed work
retains ongoing work. It records no payloads, raw errors, credentials or file paths.
Game destination controls use public HTTPS Roblox game URLs or safe positive IDs;
private invite links belong in Private Servers. Recent destinations retain only
place IDs and public game names, are limited to 20 entries, and live in memory.
