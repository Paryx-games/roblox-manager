# RM Design System

Status: **v1 - dark mode only.** Light mode is not yet specified; do not add
one ad hoc, and do not add a `light:` or `dark:` Tailwind variant anywhere in
the app. There is exactly one mode until this doc says otherwise.

This doc is normative, not descriptive. Every "must" is enforced - see
**§11 Enforcement** for how, and for what is deliberately _not_ mechanically
enforced. Where a rule includes a number, that number is the rule. "Use
good judgment" is never load-bearing on its own in this doc - every escape
valve below is bounded by an explicit test, not a vibe.

Stack assumption for everything below: **React + Tailwind, utility classes
generated from tokens** (see §3.1). No component in this app writes a raw
`style={{ ... }}` with a literal value, and no component reaches for a
Tailwind color/spacing/radius utility that isn't backed by a token in
§3.1's table. If Tailwind's default scale would produce a value not in that
table, the utility is wrong, not the rule.

---

## Contents

1. Concept
2. Layout shell
3. Tokens (the actual reference table - start here for any styling question)
4. Type
5. Component contracts
6. Interaction states
7. `Card` vs `DataTable` vs plain list
8. Responsive behavior
9. Icons
10. Writing in the UI
11. Enforcement
12. Anti-patterns

---

## 1. Concept

RM is an operations console for people running many Roblox accounts and game
instances at once - closer to a process manager or an IDE than a marketing
dashboard. The design borrows VS Code's _discipline_, not its literal
chrome: a fixed activity bar + sidebar + content shell, status communicated
through color and never through decoration, and **high information density
with deliberate spacing** - not cramped, not padded out for its own sake,
but never decorative whitespace either.

Four rules fall out of that concept and everything else in this doc exists
to protect them:

1. **Status color is reserved.** `--status-online/warning/danger/neutral`
   may only be used to represent live account/instance/process state (a
   presence dot, a launch-progress badge, an error row). Never on buttons,
   links, decoration, or emphasis. If nothing about the pixel represents
   live state, it doesn't get a status color.
2. **No shadows, ever.** Depth comes only from the three-step background
   scale (`--bg-canvas` → `--bg-surface` → `--bg-raised`). `box-shadow` is
   not in the token file, not in the Tailwind config's `boxShadow` theme
   key, and must not appear in a component - with exactly one exception,
   defined in §6.
3. **One radius, one border weight, everywhere.** `--radius` (6px) and
   `--border-width` (1px) are used on every rounded/bordered element in the
   app, full stop. A component that needs a different radius to "feel
   right" is a signal to revisit the component, not to add a token.
4. **Components consume semantic tokens only.** `bg-action-primary`, never
   `bg-[#3b82f6]`. This is what lets the whole palette move later (e.g. a
   light mode) without touching component code.

---

## 2. Layout shell (fixed - not a per-page choice)

```
┌──┬──────────────┬───────────────────────────────────────────┐
│  │ RM            │  ● online: 12   ⧗ launching: 2      [··] │  titlebar (40px)
├──┼──────────────┼───────────────────────────────────────────┤
│▪ │ ACCOUNTS      │  Page Title                                │
│▪ │ INSTANCES     │  One-line description                      │
│▪ │ GROUPS        │  ─────────────────────────────────────    │
│▪ │ ASSETS        │                                             │
│▪ │ ACTIVITY      │  [page content]                            │
│  │               │                                             │
│⚙ │ SETTINGS      │                                             │
└──┴──────────────┴───────────────────────────────────────────┘
 44px   220px              flex-1, content capped at 960px, left-aligned
```

- Activity bar (44px) and sidebar (220px) are **fixed widths** at the
  comfortable breakpoint - see §8 for what changes below it.
- Sidebar's 7 top-level items are the _only_ place uppercase/tracked type
  appears anywhere in the app. Zero other exceptions (§12).
- Content area caps at `--content-max-width` (960px) and left-aligns.
  Nothing in RM is center-aligned marketing content - every page reads
  top-left to bottom-right.
- Every page's content region starts with `<PageHeader title description />`
  (§5) - page title (`text-xl`, sans), one-line description (`text-sm`,
  `text-muted`, sans), then a `border-default` hairline rule. Pages compose
  this component; they do not hand-roll a title block.

---

## 3. Tokens

This is the single source of truth for every value used in the app. If a
value isn't in one of these tables, it doesn't go in a component - either
it's missing from this doc (fix the doc first) or it shouldn't exist.

### 3.1 Color

| Token              | Tailwind utility (examples)                 | Value       | Usage                                                        |
| ------------------ | ------------------------------------------- | ----------- | ------------------------------------------------------------ |
| `--bg-canvas`      | `bg-canvas`                                 | `#0d0e11`   | App background, page base layer                              |
| `--bg-surface`     | `bg-surface`                                | `#151619`   | Cards, sidebar, table rows (default)                         |
| `--bg-raised`      | `bg-raised`                                 | `#1e2024`   | Hover/active step, popovers, modals                          |
| `--text-primary`   | `text-primary`                              | `#e8e9ec`   | Body copy, headings, values                                  |
| `--text-muted`     | `text-muted`                                | `#8b8d94`   | Descriptions, secondary labels, placeholder text             |
| `--text-disabled`  | `text-disabled`                             | `#55575e`   | Disabled control labels                                      |
| `--border-default` | `border-default`                            | `#2a2c31`   | Hairlines, table borders, card borders, dividers             |
| `--action-primary` | `bg-action-primary` / `text-action-primary` | `#3b82f6`   | Primary button, links, focus ring, active nav indicator      |
| `--action-muted`   | `bg-action-muted`                           | `#3b82f622` | Selected row/active-accent background (14% alpha of primary) |
| `--status-online`  | `bg-status-online` / `text-status-online`   | `#3ecf6b`   | Live "online" state only - see §1 rule 1                     |
| `--status-warning` | `bg-status-warning` / `text-status-warning` | `#e2a63b`   | Live "warning" state only                                    |
| `--status-danger`  | `bg-status-danger` / `text-status-danger`   | `#e5484d`   | Live "danger/error" state only                               |
| `--status-neutral` | `bg-status-neutral` / `text-status-neutral` | `#6b6d75`   | Live "idle/offline/unknown" state only                       |

No other color token exists. A component that needs a color not on this
list is a signal to add the token here first (PR checklist §11.3), never to
inline a hex.

### 3.2 Spacing

Tailwind's default spacing scale is **restricted** to this subset. Do not
use a Tailwind spacing utility outside this list (`p-5`, `gap-7`, `m-11`,
etc. are all invalid - they don't exist in this scale on purpose).

| Token        | Tailwind value | px   |
| ------------ | -------------- | ---- |
| `--space-1`  | `1`            | 4px  |
| `--space-2`  | `2`            | 8px  |
| `--space-3`  | `3`            | 12px |
| `--space-4`  | `4`            | 16px |
| `--space-6`  | `6`            | 24px |
| `--space-8`  | `8`            | 32px |
| `--space-12` | `12`           | 48px |

**Structural exception** (§7's original escape valve, now bounded): a value
outside this scale is allowed _only_ when it matches one of these three
named cases, and a comment must cite which case:

1. A fixed shell dimension: `44px` (activity bar / icon square), `220px`
   (sidebar), `40px` (titlebar), `960px` (content max-width).
2. A third-party component's required prop (e.g. a chart library's fixed
   legend height) - cite the library and prop name in the comment.
3. A 1px hairline used as a border, not spacing (borders aren't on the
   spacing scale at all; they're `--border-width`).

"This looked better at 18px than 16px" is **not** a structural reason and
does not qualify for the exception under any circumstance.

### 3.3 Radius & border

| Token            | Value | Usage                                 |
| ---------------- | ----- | ------------------------------------- |
| `--radius`       | 6px   | Every rounded element, no exceptions  |
| `--border-width` | 1px   | Every bordered element, no exceptions |

There is no `--radius-sm` / `--radius-lg`. If a design calls for one,
the design is wrong per §1 rule 3, not the token file.

### 3.4 Typography scale

| Token         | Tailwind    | px / line-height | Family                |
| ------------- | ----------- | ---------------- | --------------------- |
| `--text-xs`   | `text-xs`   | 11px / 16px      | sans or mono (see §4) |
| `--text-sm`   | `text-sm`   | 13px / 20px      | sans or mono          |
| `--text-base` | `text-base` | 14px / 20px      | sans or mono          |
| `--text-lg`   | `text-lg`   | 16px / 24px      | sans only             |
| `--text-xl`   | `text-xl`   | 20px / 28px      | sans only             |

No inline `font-size`, no Tailwind arbitrary-value text utility
(`text-[15px]`) anywhere.

### 3.5 Motion

| Token             | Value                        | Usage                                    |
| ----------------- | ---------------------------- | ---------------------------------------- |
| `--duration-fast` | 100ms                        | Hover/active background step, focus ring |
| `--duration-base` | 160ms                        | Panel/sidebar collapse, tooltip          |
| `--duration-slow` | 240ms                        | Modal/popover enter                      |
| `--ease-standard` | `cubic-bezier(0.2, 0, 0, 1)` | All of the above                         |

No other easing curve, no spring physics, no duration not on this list. See
§12 for the motion anti-patterns this scale exists to prevent.

### 3.6 Breakpoints

| Token                      | Value  |
| -------------------------- | ------ |
| `--breakpoint-comfortable` | 1200px |
| `--breakpoint-condensed`   | 768px  |

---

## 4. Type

Two families, each with a fixed job. Don't cross them.

| Family                | Token         | Used for                                                                                                        |
| --------------------- | ------------- | --------------------------------------------------------------------------------------------------------------- |
| Sans (Inter)          | `--font-sans` | Headings, body copy, descriptions, button labels, nav labels, empty-state/error copy                            |
| Mono (JetBrains Mono) | `--font-mono` | Account names/usernames, IDs, place/job IDs, timestamps, table cell values, status badges, log/activity entries |

The test, applied identically every time: **if the value came from data**
(Roblox, the account store, a process) **it's mono. If it's UI chrome you
wrote, it's sans.** No third case. If a value is genuinely ambiguous
(rare), it's mono - data-adjacent defaults to mono, not sans.

---

## 5. Component contracts

Every reusable visual element ships as one component in `src/components/`
with a single, documented job - not inline JSX styled per-page. A page
composes components; it does not define its own button, badge, or table row
styling.

### 5.1 When a new component is justified

A component earns its existence when **either**:

- The same visual/interaction shape appears in **2 or more places** in the
  app (not "might appear later" - actually appears, today, in the diff or
  the existing codebase), **or**
- It wraps a named abstraction with real logic or a real contract - state
  handling, a11y behavior, or a prop API that would otherwise be
  duplicated (e.g. `<StatusDot>` isn't just a colored circle, it's the
  single place that's allowed to read `--status-*` tokens).

It is **not** justified by wanting a semantic-sounding wrapper. If you're
about to write:

```tsx
<Page>
  <DashboardContainer>
    <DashboardSection>
      <DashboardSectionHeader>
        <DashboardSectionTitle>
```

- stop. Each of those is a single `<div className="...">` using tokens
  directly, used once. A plain div is correct when there's no measured reuse
  and no logic being abstracted. This is checked in review by asking "show me
  the second usage" or "what does this component do that a div + Tailwind
  classes doesn't" - if there's no answer, it's a div.

### 5.2 Minimum set for v1

Each backed by tokens only, each with a single default export:

- `<PageHeader title description />`
- `<Sidebar />` / `<ActivityBarIcon />` - fixed, see §2
- `<StatusDot status="online|warning|danger|neutral" />` - the _only_
  component allowed to read `--status-*` tokens for anything beyond a table
  row's left border or a badge background
- `<DataTable>` - for listy/tabular content. See §7 for exactly when.
- `<Card>` - for grouped, non-listy content. See §7.
- `<Button variant="primary|secondary|danger" />` - see §6 for required states
- `<Badge>` - mono type, for IDs/short data tags
- `<EmptyState>` / `<ErrorState>` - see §10 writing guidance, §7.3 for
  where `DataTable` must use these

A new component must be added to this list (and this doc) in the same PR
that introduces it - §11.3.

---

## 6. Interaction states

Every interactive component defines behavior for **every** state in its row
below - not just default/hover. No partial credit; a missing state is a
review-blocking gap, not a nit.

| Component        | Required states                                                       |
| ---------------- | --------------------------------------------------------------------- |
| Button           | default, hover, active, focus, disabled, loading                      |
| Input/select     | default, hover, focus, disabled, error                                |
| Table row        | default, hover, selected                                              |
| Sidebar/nav item | default, hover, active (current page), focus                          |
| Icon-only button | default, hover, focus, disabled + **always** an accessible label (§9) |

Rules that apply across all of them, with no exceptions clause left open:

- **Focus is never removed, and there is exactly one accepted focus
  treatment:** `outline: 2px solid var(--action-primary)` with
  `outline-offset: 2px`. Tailwind: `outline outline-2 outline-offset-2
outline-action-primary`. `outline: none` without this exact replacement
  fails review. There is no "or equivalent" clause - if `outline` is
  visually clipped by a parent with `overflow: hidden`, the fix is to give
  that parent `overflow: visible` on the focus container, or restructure
  the DOM - **not** to invent a substitute treatment. The sole permitted
  substitute, only when outline genuinely cannot render (documented case:
  an element inside a `overflow: hidden` scroll container that cannot be
  restructured) is `box-shadow: 0 0 0 2px var(--action-primary)` - this is
  the **one and only** exception to "no box-shadow, ever" in §1 rule 2, and
  the component's code comment must say which case applies.
- **Disabled means visibly disabled:** `text-disabled` for label text,
  `cursor-not-allowed`, and no hover/active background transition. Reduced
  opacity alone is never sufficient by itself.
- **Loading states use §3.5 motion tokens only** - no component-specific
  animation duration invented on the spot.
- Hover/active background changes step **exactly one level** on
  `bg-canvas` → `bg-surface` → `bg-raised`, or use `bg-action-muted` for
  selected/active-accent states - never an arbitrary opacity or custom
  color.

---

## 7. `Card` vs `DataTable` vs plain list - decision tree

```
Is this repeated data (accounts, instances, activity entries)?
        │
       yes ──────────────────────────────► no
        │                                   │
Does it have ≥2 comparable fields           Is it grouped
beyond a name/label (status, last-active,   configuration or
owner, size, etc.)? A single action         freeform content?
button does NOT count as a field.                 │
        │                                         ↓
   ┌────┴────┐                                   Card (§5.2)
  yes        no
   ↓          ↓
DataTable   plain list
(§5.2)      (simple <ul>-like stack,
             tokens only, no new
             component unless it hits
             the §5.1 reuse threshold)
```

**The ≥2-field threshold is the rule, not a guideline.** If item count is
likely to exceed 5 regardless of field count, default to `DataTable` -
density over a card grid is the concept-level default (§1). "It felt more
like a list" is not a valid reason to skip `DataTable` once the threshold
is met.

### 7.3 `DataTable` loading / empty / error contract

`DataTable` owns all three of these states internally - a page never
hand-rolls a loading spinner or empty message inside a table region:

| State   | Behavior                                                        |
| ------- | --------------------------------------------------------------- |
| Loading | Skeleton rows (3-5, matching column count) using `bg-surface` → |

             `bg-raised` pulse on `--duration-slow`, never a full-page spinner swap |

| Empty | Table chrome (header row) stays, body region renders `<EmptyState>` |
| Error | Table chrome stays, body region renders `<ErrorState>` with a retry action if the fetch is retriable |

---

## 8. Responsive behavior

RM is desktop-first but the window is resizable - specify behavior rather
than letting it happen by accident:

| Width                                        | Behavior                                                                                               |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| ≥ `--breakpoint-comfortable` (1200px)        | Full layout as specified in §2.                                                                        |
| `--breakpoint-condensed`-1199px (768-1199px) | Sidebar stays, content area narrows; `DataTable` drops lower-priority columns before wrapping text.    |
| < `--breakpoint-condensed` (768px)           | Sidebar collapses to the activity bar only (icons, no labels) - same collapsed state as manual toggle. |

No separate mobile layout is in scope - the floor is "doesn't break," not
"optimized for a phone-sized window." Column drop-priority for `DataTable`
must be declared per-table (a `priority` prop per column, lowest dropped
first) - silently reflowing columns in arbitrary order is not acceptable.

---

## 9. Icons

- One icon set only: `assets/icons` (svg/png, per AGENTS.md). No second
  icon library for "just this one icon."
- Icon color follows §3.1: `text-primary` / `text-muted` / `text-disabled`
  for ordinary icons. Icons only carry a `--status-*` color when they _are_
  a status indicator - an icon next to a status row doesn't inherit the
  status color just because it's nearby.
- **No emoji as UI icons, anywhere, ever.**
- Icon-only buttons require an accessible label (`aria-label` or
  equivalent) - not optional, not covered by a tooltip alone.

---

## 10. Writing in the UI

- Buttons name the action they perform: "Launch instance," not "Submit" or
  "Go." A toast confirming it uses the same verb: "Instance launched."
- Errors state what happened and, where possible, what to do - never
  apologize, never say "something went wrong" without detail if a more
  specific cause is known.
- Empty states are an invitation to act ("No accounts yet - add one to get
  started"), not just an absence notice.

---

## 11. Enforcement

A doc alone will not hold under PR volume - but over-enforcement is its own
failure mode. Enforcement is split deliberately by how mechanical the rule
actually is.

### 11.1 Stylelint / ESLint - hard, unambiguous rules only

CI-blocking:

- Raw hex colors outside `tokens.css` / the Tailwind theme config.
- Any Tailwind arbitrary-value bracket syntax for color, spacing, radius,
  or font-size (`bg-[#...]`, `p-[18px]`, `rounded-[4px]`, `text-[15px]`) -
  banned via `eslint-plugin-tailwindcss`'s `no-arbitrary-value` rule.
- `box-shadow` outside the one documented exception in §6 (grepped for a
  required `// focus-outline-exception: <case>` comment; unlabeled
  `box-shadow` fails).
- `border-radius` / `rounded-*` values other than `--radius` / `rounded-md`
  (mapped 1:1 to 6px - no other radius scale entries exist in the Tailwind
  config, so `rounded-lg`/`rounded-full` etc. simply aren't available).
- `font-family` not referencing the `sans` / `mono` theme keys.
- Tailwind spacing utilities outside the restricted scale in §3.2 - enforced
  by removing every other spacing key from the Tailwind theme config, not
  by a regex (a value that isn't configured can't be generated).

### 11.2 A visual component gallery, not just a doc

A `/dev/design-system` route (dev-build only) renders every component in
§5.2 with every state from §6 side by side. This is the reference
contributors screenshot and copy from. Kept in sync via the PR checklist.

### 11.3 PR template checklist

```markdown
- [ ] Any new UI uses existing components from `src/components/` and
      tokens from §3 - no raw hex/shadow/radius, and no off-scale
      spacing without one of the three §3.2 structural exceptions
      cited in a comment.
- [ ] If I added a new reusable component, it passes the §5.1 test
      (2+ real usages, or a real abstraction), is documented in this
      file's §5.2, includes the §6 states that apply to it, and is
      added to `/dev/design-system`.
- [ ] If I added a table, it follows the §7.3 loading/empty/error
      contract instead of a hand-rolled state.
```

This is the weakest layer (checkboxes get rubber-stamped) but it catches
_taste_ violations lint can't - a technically token-compliant component
that still reinvents a card style, or a button missing a focus state lint
has no way to detect.

### 11.4 For AI agents specifically

Read `tokens.css` (or the Tailwind theme config) and this doc in full
before any UI change - same as the docx/pptx skills being required reads
before those file types. An agent that hasn't read §12 will confidently
generate the soft-shadow rounded-card kit with emoji icons and no focus
states, because that combination _is_ the statistical default for
"dashboard UI." Naming the anti-patterns explicitly overrides that default.
Where this doc gives a bounded exception (§3.2 structural spacing, §6's
single box-shadow case), treat the bound as exhaustive - do not extend it
by analogy to a case not listed. If a new case seems to genuinely need an
exception, propose adding it to this doc in the same PR rather than taking
it silently.

---

## 12. Anti-patterns (things that will pass a compiler but fail review)

- ❌ A card with `border-radius` different from `--radius`, or any
  `box-shadow` outside the single §6 focus exception.
- ❌ Uppercase-tracked labels outside the sidebar's 7 top-level items.
- ❌ A status color used on a button, link, page decoration, or a
  non-status icon.
- ❌ A raw hex value, or a `font-family` not pulled from §3, anywhere in a
  component file.
- ❌ A spacing or font-size value not on the §3.2/§3.4 scale, without a
  cited §3.2 structural exception.
- ❌ Center-aligned page content, or a marketing-style hero/stat-callout on
  an internal page.
- ❌ A card-grid layout for data that meets the §7 ≥2-field `DataTable`
  threshold.
- ❌ A component that fails the §5.1 justification test.
- ❌ A button, input, or nav item missing a required §6 state - most
  commonly, a missing or non-standard focus ring.
- ❌ Emoji used as an icon (§9).
- ❌ Decorative animation: hover scale/glow, bounce/overshoot easing,
  scroll-triggered reveals, or any duration/easing not in §3.5.
- ❌ A hand-rolled loading/empty/error state inside a `DataTable` instead
  of the §7.3 contract.
