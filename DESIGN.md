# RM Design System

Status: **v1 - dark mode only.** Light mode is not yet specified; do not add one ad hoc.

This doc is normative, not descriptive. "Should" and "must" below are enforced -
see **Enforcement** at the bottom for how, and for what is deliberately _not_
mechanically enforced.

---

## 1. Concept

RM is an operations console for people running many Roblox accounts and game
instances at once - closer to a process manager or an IDE than a marketing
dashboard. The design borrows VS Code's _discipline_, not its literal chrome:
a fixed activity bar + sidebar + content shell, status communicated through
color and never through decoration, and **high information density with
deliberate spacing** - not cramped, not padded out for its own sake, but
never decorative whitespace either.

Four rules fall out of that concept and everything else in this doc exists
to protect them:

1. **Status color is reserved.** `--status-online/warning/danger/neutral`
   may only be used to represent live account/instance/process state (a
   presence dot, a launch-progress badge, an error row). They must never be
   used for buttons, links, decoration, or emphasis. If nothing about the
   pixel represents live state, it doesn't get a status color.
2. **No shadows, ever.** Depth comes only from the three-step background
   scale (`--bg-canvas` → `--bg-surface` → `--bg-raised`). `box-shadow` is
   not in the token file and should not be added to a component.
3. **One radius, one border weight, everywhere.** `--radius` (6px) and
   `--border-width` (1px) are used on every rounded/bordered element in the
   app, full stop. A component that needs a different radius to "feel
   right" is a signal to revisit the component, not to add a token.
4. **Components consume semantic tokens only.** `--action-primary`, not a
   raw hex. This is what lets the whole palette move later (e.g. a light
   mode) without touching component code - see tokens.css's note on
   semantic layering.

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

- Activity bar (44px) and sidebar (220px) are **fixed widths** in the
  comfortable breakpoint - see §7 for what changes below it.
- Sidebar labels are the _only_ place uppercase/tracked type is used in the
  whole app. Do not use uppercase-tracked labels anywhere else (see §8
  Anti-patterns).
- Content area caps at `--content-max-width` and left-aligns. Nothing in RM
  is center-aligned marketing content - every page reads top-left to
  bottom-right.
- Every page's content region starts with the same header block: page
  title (`--text-xl`, sans), one-line description (`--text-sm`, muted,
  sans), then a `--border-default` hairline rule. This is the
  `<PageHeader>` component (§4) - pages must use it, not hand-roll a title.

---

## 3. Type

Two families, each with a fixed job. Don't cross them.

| Family                | Token         | Used for                                                                                                        |
| --------------------- | ------------- | --------------------------------------------------------------------------------------------------------------- |
| Sans (Inter)          | `--font-sans` | Headings, body copy, descriptions, button labels, nav labels, empty-state/error copy                            |
| Mono (JetBrains Mono) | `--font-mono` | Account names/usernames, IDs, place/job IDs, timestamps, table cell values, status badges, log/activity entries |

Rule of thumb: **if it's a value that came from data (Roblox, the account
store, a process), it's mono. If it's UI chrome you wrote, it's sans.** Same
test every time.

Scale is fixed at `--text-xs/sm/base/lg/xl`. No inline `font-size` outside
that list.

---

## 4. Component contracts

Every reusable visual element ships as one component in `src/components/`
with a single, documented job - not as inline JSX styled per-page. A page
composes components; it does not define its own button, badge, or table row
styling.

**Do not create a component solely to wrap a single element or a one-off
piece of layout.** A component earns its existence by representing a
reusable pattern or a meaningful abstraction - not by giving every `<div>`
a name. If you're about to write:

```tsx
<Page>
  <DashboardContainer>
    <DashboardSection>
      <DashboardSectionHeader>
        <DashboardSectionTitle>
```

stop - that's HTML wearing a costume. A plain `<div className="...">` using
tokens directly is correct when there's no real reuse or abstraction
happening. Reach for a named component when the same shape appears in more
than one place, or when naming it genuinely clarifies intent.

Minimum set for v1, each backed by tokens only:

- `<PageHeader title description />`
- `<Sidebar />` / `<ActivityBarIcon />` - fixed, see §2
- `<StatusDot status="online|warning|danger|neutral" />` - the _only_
  component allowed to consume `--status-*` tokens for anything other than
  a table row's left border or a badge background
- `<DataTable>` - for listy/tabular content. See §6 for exactly when.
- `<Card>` - for grouped, non-listy content. See §6.
- `<Button variant="primary|secondary|danger" />` - see §5 for required states
- `<Badge>` - mono type, for IDs/short data tags
- `<EmptyState>` / `<ErrorState>` - see §9 writing guidance

A new component must be added to this list (and this doc) in the same PR
that introduces it - see Enforcement §3.

---

## 5. Interaction states

Every interactive component must define behavior for each state that
applies to it - not just its default/hover appearance. This is the state
set to check against per component type:

| Component        | Required states                                                           |
| ---------------- | ------------------------------------------------------------------------- |
| Button           | default, hover, active, focus, disabled, loading                          |
| Input/select     | default, hover, focus, disabled, error                                    |
| Table row        | default, hover, selected                                                  |
| Sidebar/nav item | default, hover, active (current page), focus                              |
| Icon-only button | default, hover, focus, disabled + **always** an accessible label (see §8) |

Rules that apply across all of them:

- **Focus is never removed.** Every focusable element gets a visible focus
  ring using `--action-primary` at `--border-width * 2` (2px) outline, or
  an equivalent visible treatment. Never `outline: none` without a
  replacement that's at least as visible.
- **Disabled means visibly disabled.** `--text-disabled` for label text,
  reduced opacity is not sufficient on its own - pair it with `cursor:
not-allowed` and no hover/active transition.
- **Loading states use motion tokens** (§ tokens.css Motion), not a
  component-specific animation duration invented on the spot.
- Hover/active background changes step exactly one level on the
  `--bg-canvas` → `--bg-surface` → `--bg-raised` scale, or use
  `--action-muted` for selected/active-accent states - never an
  arbitrary opacity tweak.

---

## 6. `Card` vs `DataTable` vs plain list - decision tree

```
Is this repeated data (accounts, instances, activity entries)?
        │
       yes ──────────────────────────────► no
        │                                   │
Does it have columns / comparable           Is it grouped
fields across items (status, name,          configuration or
last-active, actions)?                      freeform content
        │                                   (e.g. a settings
   ┌────┴────┐                              section)?
  yes        no                                   │
   ↓          ↓                                   ↓
DataTable   plain list                          Card
(§4)        (simple <ul>-like                  (§4)
             stack, tokens only,
             no new component
             unless truly reused)
```

If in doubt and the item count is likely to exceed ~5, default to
`DataTable` - density over a card grid is the concept-level default (§1).

---

## 7. Responsive behavior

RM is desktop-first but the window is resizable - specify behavior rather
than letting it happen by accident:

| Width                                        | Behavior                                                                                                                               |
| -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| ≥ `--breakpoint-comfortable` (1200px)        | Full layout as specified in §2.                                                                                                        |
| `--breakpoint-condensed`-1199px (768-1199px) | Sidebar stays, content area narrows; `DataTable` drops lower-priority columns before wrapping text.                                    |
| < `--breakpoint-condensed` (768px)           | Sidebar collapses to the activity bar only (icons, no labels) - same collapsed state as a manually toggled sidebar, not a new pattern. |

No separate mobile layout is in scope - this is a desktop app; the floor is
"doesn't break," not "optimized for a phone-sized window."

---

## 8. Icons

- One icon set only: the existing `assets/icons` (svg/png, per AGENTS.md).
  Don't introduce a second icon library for "just this one icon."
- Icon color follows the same rule as everything else: `--text-primary` /
  `--text-muted` / `--text-disabled` for ordinary icons. Icons only carry a
  `--status-*` color when they _are_ a status indicator (e.g. a presence
  dot) - an icon next to a status row doesn't inherit the status color
  just because it's nearby.
- **No emoji as UI icons**, anywhere, ever - not in the sidebar, not in
  buttons, not in empty states. Emoji render inconsistently across
  platforms and read as unstyled placeholder content, not as this system's
  icon language.
- Icon-only buttons (no visible text label) require an accessible label
  (`aria-label` or equivalent) - this is not optional and not covered by a
  tooltip alone.

---

## 9. Writing in the UI

- Buttons name the action they perform: "Launch instance," not "Submit" or
  "Go." A toast confirming it uses the same verb: "Instance launched."
- Errors state what happened and, where possible, what to do - never
  apologize, never say "something went wrong" without detail if a more
  specific cause is known.
- Empty states are an invitation to act ("No accounts yet - add one to get
  started"), not just an absence notice.

---

## 10. Anti-patterns (things that will pass a compiler but fail review)

- ❌ A card with `border-radius` different from `--radius`, or any
  `box-shadow`.
- ❌ Uppercase-tracked labels outside the sidebar's 7 top-level items.
- ❌ A status color (`--status-*`) used on a button, link, or as page
  decoration, or on a non-status icon.
- ❌ A raw hex value, or a `font-family` not pulled from tokens.css,
  anywhere in a component file.
- ❌ A spacing or font-size value not on the declared scale.
- ❌ Center-aligned page content, or a marketing-style hero/stat-callout
  treatment on an internal page.
- ❌ A card-grid layout for tabular/listy data instead of `DataTable`
  (§6).
- ❌ A component that wraps one element with no reuse and no clarifying
  abstraction (§4).
- ❌ A button, input, or nav item missing a required interaction state
  from §5 - most commonly, a missing focus ring.
- ❌ Emoji used as an icon (§8).
- ❌ Decorative animation: hover scale/glow, bounce/overshoot easing,
  scroll-triggered reveals, or a page transition nobody asked for (§ tokens
  Motion).

---

## Enforcement

A doc alone will not hold under PR volume - but over-enforcement is its own
failure mode. **The system should constrain design decisions, not turn
ordinary development into an obstacle course.** If a contributor (human or
agent) has to fight the linter for twenty minutes to add a legitimate 1px
divider or a genuinely structural `44px`, the tooling has become the thing
that gets worked around, which is worse than not having it. So enforcement
is split deliberately by how mechanical the rule actually is:

### 1. Stylelint - hard, unambiguous rules only

Lint-enforced (CI-blocking, same spirit as `cargo fmt -- --check` already
blocking merges in this repo):

- Raw hex colors outside `tokens.css`.
- `box-shadow`, anywhere, full stop.
- `border-radius` values other than `var(--radius)`.
- `font-family` not referencing `var(--font-sans)` / `var(--font-mono)`.

**Deliberately NOT lint-enforced** - flagged in code review / the PR
checklist instead, because the rule requires judgment a regex can't apply
without false positives on legitimate structural sizing (§2's fixed
44px/220px/40px, a one-off `44px` icon square, etc.):

- Spacing values off the `--space-*` scale.
- Font sizes off the `--text-*` scale.

If this trade-off causes real drift in practice, tighten it later with an
allowlist (structural/layout files exempted) rather than loosening the hard
rules above.

### 2. A visual component gallery, not just a doc

A `/dev/design-system` route (dev-build only) that renders every component
in §4 with every state from §5 side by side. This is the actual reference
contributors screenshot and copy from - prose is easy to skim past, a
rendered page of "here is the only Button you're allowed to build, in every
state" is not. Keep it in sync via the PR checklist (§3 below).

### 3. PR template checklist item

```markdown
- [ ] Any new UI uses existing components from `src/components/` and
      tokens from `tokens.css` - no raw hex/shadow/radius, and no
      off-scale spacing without a structural reason.
- [ ] If I added a new reusable component, it's documented in
      `DESIGN.md` §4, includes the interaction states from §5
      that apply to it, and is added to `/dev/design-system`.
```

This is the weakest layer (checkboxes get rubber-stamped) but it's the one
that catches _taste_ violations lint can't - a technically token-compliant
component that still reinvents a card style, or a button that's missing a
focus state lint has no way to detect.

### For AI agents specifically

Point agents at `tokens.css` and this doc explicitly in AGENTS.md as a
required read before any UI change - same as the docx/pptx skills being
required reads before those file types. An agent that hasn't read §10
Anti-patterns will confidently generate the soft-shadow rounded-card kit
with emoji icons and no focus states, because that combination _is_ the
statistical default for "dashboard UI." Naming the anti-patterns explicitly
is what overrides that default - and pointing out explicitly that
enforcement is intentionally partial (§Enforcement) stops an agent from
either fighting the linter over a structural value or, worse, assuming
nothing outside the linter's reach matters.
