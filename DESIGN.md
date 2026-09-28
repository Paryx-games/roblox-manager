# RM interface guide

This document describes the active Tauri + React interface and sets the shared
visual conventions for future changes. It is based on the current v2 source.
The current interface is dark mode only.

## Source files

- `ram_ui/frontend/` contains the React pages and shared components.
- `ram_ui/frontend/styles.css` imports Tailwind CSS 4, imports the shared
  token file, defines the Tailwind theme mapping, and contains the app's page
  and component styles.
- `ram_ui/src/tokens.css` defines the shared color, type, spacing, radius,
  motion, and shell values. Its location is historical; the active frontend
  imports it from `styles.css`.
- `ram_ui/.stylelintrc.json` and the package scripts define automated
  frontend linting.

Most of the interface uses named CSS classes. Tailwind utilities are used in
the app shell and for a small number of mapped theme values. Do not assume
that every token has a matching Tailwind utility.

## Window and navigation

The main Tauri window opens at 1200 x 760 and cannot be resized below that
size. It has a 40px title bar, a 56px icon navigation rail, and a flexible
main content area. The settings page and some workspaces have their own
internal sidebars. Their widths belong to those pages, not the app shell.

The frontend stylesheet includes media rules for narrower content, but a
window below the configured minimum is not a supported main-window size.
Window dimensions are CSS logical pixels; Windows and WebView2 apply display
scaling.

## Shared visual tokens

Use `var(--token-name)` for shared visual values. The current token values
are listed in `ram_ui/src/tokens.css`; update that file when a shared value
changes.

| Purpose | Tokens |
| --- | --- |
| Surfaces | `--bg-canvas`, `--bg-surface`, `--bg-raised` |
| Text | `--text-primary`, `--text-muted`, `--text-disabled` |
| Borders and actions | `--border-default`, `--action-primary`, `--action-muted` |
| Live status | `--status-online`, `--status-warning`, `--status-danger`, `--status-neutral` |
| Other semantic states | `--notification-*`, `--access-*`, `--ownership-*` |
| Typography | `--font-sans`, `--text-xs`, `--text-sm`, `--text-base`, `--text-lg`, `--text-xl` |
| Spacing | `--space-1`, `--space-2`, `--space-3`, `--space-4`, `--space-6`, `--space-8`, `--space-12` |
| Shape and motion | `--radius`, `--border-width`, `--duration-*`, `--ease-standard` |

Use Roboto for interface text. The icon component loads the project's local
icon set; use it for interface actions instead of emoji or one-off icon
libraries. Keep live account and process status distinct from static access
permissions, notification severity, and private-server ownership.

## Existing shared components

Reuse a component when it already covers the needed behavior:

- `Icon`, `AccountAvatar`, and `AccountPicker` for common account and icon UI.
- `Select` for the custom keyboard-operable selection menu.
- `Popup`, `PopupMenu`, and `PromptModal` for dialogs, action menus, and text prompts.
- `TooltipProvider` for shared tooltips.
- `LoadingSkeleton` for pending page and data states.
- `Toast` for operation feedback.

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

The app responds to `prefers-reduced-motion: reduce`. Keep transitions tied
to the user's action, and ensure page changes and loading indicators remain
understandable when motion is reduced. Menus and dialogs should support their
existing keyboard dismissal and focus behavior.

## CSS and lint

Run `pnpm --dir ram_ui lint` after CSS or component changes. ESLint checks
the TypeScript source. Stylelint rejects hex colors and `!important` in
frontend CSS and applies the standard CSS rules configured in
`ram_ui/.stylelintrc.json`. The linter does not verify the full design
system, keyboard behavior, contrast, or responsive appearance; review those
in the affected interface.

Use token colors, Roboto, the shared radius, and existing focus treatments.
Avoid shadows and new one-off visual patterns. Keep CSS near the current
page styles unless multiple pages share the same visual behavior.

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
