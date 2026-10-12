import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Icon } from "./Icon";
import { PopupMenu } from "./PopupMenu";
import { TagSigns } from "./TagSigns";
import type { AccountTag } from "../lib/ipc";

export function TagPicker({
  tags,
  selected,
  onChange,
  disabled = false,
  label,
  emptyLabel = "No tags",
  clearLabel = "Clear tags",
  onManage,
  mixed = [],
  busy = false,
  signs = false,
  showSingleName = false,
  matchAll,
  onMatchAllChange,
  onSelectMatching,
  canSelectMatching = true,
}: {
  tags: AccountTag[];
  selected: string[];
  onChange: (ids: string[]) => void;
  disabled?: boolean;
  label: string;
  emptyLabel?: string;
  clearLabel?: string;
  onManage?: () => void;
  mixed?: string[];
  busy?: boolean;
  signs?: boolean;
  showSingleName?: boolean;
  matchAll?: boolean;
  onMatchAllChange?: (value: boolean) => void;
  onSelectMatching?: () => void;
  canSelectMatching?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [above, setAbove] = useState(false);
  const container = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement | null>(null);
  const restoreFocus = useRef(false);
  const options = [
    ...tags,
    ...selected
      .filter((id) => !tags.some((tag) => tag.id === id))
      .map((id) => ({
        id,
        name: "Missing tag",
        color: [107, 112, 128] as [number, number, number],
      })),
  ];
  const names = selected.map(
    (id) => options.find((tag) => tag.id === id)?.name ?? "Missing tag",
  );
  const summary =
    names.length > 1
      ? `${names[0]} +${names.length - 1}`
      : (names[0] ?? emptyLabel);

  useEffect(() => {
    if (open && !options.length && !onManage) {
      menu.current
        ?.querySelector<HTMLElement>("button:not([disabled])")
        ?.focus();
    }
  }, [open, options.length, onManage]);

  useEffect(() => {
    if (open || disabled || !restoreFocus.current) return;
    // a save can disable the trigger during dismissal; keep later user focus intact
    if (document.activeElement === document.body) trigger.current?.focus();
    restoreFocus.current = false;
  }, [open, disabled]);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      if (
        event.target instanceof Node &&
        !container.current?.contains(event.target)
      )
        setOpen(false);
    };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [open]);

  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const bounds = trigger.current?.getBoundingClientRect();
      if (!bounds) return;
      const height = Math.min(240, window.innerHeight - 16);
      setAbove(
        bounds.bottom + 4 + height > window.innerHeight &&
          bounds.top - 4 > height,
      );
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open]);

  return (
    <div
      ref={container}
      className={`account-picker account-picker-groups account-tag-picker ${signs ? "tag-picker-signs" : ""} ${open ? "is-open" : ""}`}
      onClick={(event) => {
        if (signs) event.stopPropagation();
      }}
      onKeyDown={(event) => {
        if (signs) event.stopPropagation();
      }}
      onDragStart={(event) => {
        if (signs) {
          event.preventDefault();
          event.stopPropagation();
        }
      }}
    >
      <button
        ref={trigger}
        className={`account-picker-trigger ${signs ? "account-tag-badge" : ""} ${selected.length ? "" : "is-empty"}`}
        type="button"
        disabled={disabled}
        aria-label={`Tags for ${label}${signs ? `: ${names.length ? names.join(", ") : emptyLabel}` : ""}`}
        aria-expanded={open}
        aria-haspopup="menu"
        aria-busy={busy}
        data-tip={names.length ? names.join(", ") : emptyLabel}
        onClick={() => setOpen((current) => !current)}
      >
        {signs ? (
          <>
            <TagSigns
              tags={selected
                .map((id) => options.find((tag) => tag.id === id)!)
                .filter(Boolean)}
            />
            {showSingleName && selected.length === 1 && (
              <span className="tag-badge-name">{names[0]}</span>
            )}
          </>
        ) : (
          <>
            <span className="account-picker-left">
              <TagSigns
                tags={selected
                  .map((id) => options.find((tag) => tag.id === id)!)
                  .filter(Boolean)}
              />
              <span>{busy ? "Saving tags..." : summary}</span>
            </span>
            <Icon name="chevron-down" />
          </>
        )}
      </button>
      {open && (
        <PopupMenu
          className={`account-picker-menu ${above ? "is-above" : ""}`}
          menuRef={(element) => {
            menu.current = element;
          }}
          onClose={() => {
            restoreFocus.current = disabled;
            setOpen(false);
          }}
        >
          {!options.length && (
            <p className="tag-picker-empty">
              {onManage
                ? "No tags yet. Manage tags to create one."
                : "No tags yet. Create tags in Accounts."}
            </p>
          )}
          {options.map((tag) => (
            <button
              key={tag.id}
              type="button"
              role="menuitemcheckbox"
              aria-checked={
                selected.includes(tag.id)
                  ? true
                  : mixed.includes(tag.id)
                    ? "mixed"
                    : false
              }
              className={`account-picker-option ${selected.includes(tag.id) ? "is-selected" : ""}`}
              disabled={disabled}
              onClick={() =>
                onChange(
                  selected.includes(tag.id)
                    ? selected.filter((id) => id !== tag.id)
                    : [...selected, tag.id],
                )
              }
            >
              <span className="account-picker-checkbox" aria-hidden="true">
                {selected.includes(tag.id) ? (
                  <Icon name="check" />
                ) : mixed.includes(tag.id) ? (
                  "-"
                ) : null}
              </span>
              <span>{tag.name}</span>
              <TagSigns tags={[tag]} />
              {mixed.includes(tag.id) && !selected.includes(tag.id) && (
                <span className="tag-picker-partial">Some selected</span>
              )}
            </button>
          ))}
          <button
            type="button"
            role="menuitem"
            className="account-picker-manage"
            disabled={disabled || !selected.length}
            onClick={() => onChange([])}
          >
            {clearLabel}
          </button>
          {onMatchAllChange && selected.length > 1 && (
            <button
              type="button"
              role="menuitemcheckbox"
              aria-checked={!!matchAll}
              className="account-picker-manage"
              onClick={() => onMatchAllChange(!matchAll)}
            >
              Match all selected tags
            </button>
          )}
          {onSelectMatching && selected.length > 0 && (
            <button
              type="button"
              role="menuitem"
              className="account-picker-manage"
              disabled={!canSelectMatching}
              onClick={() => {
                setOpen(false);
                onSelectMatching();
              }}
            >
              Select matching accounts
            </button>
          )}
          {onManage && (
            <button
              type="button"
              role="menuitem"
              className="account-picker-manage"
              disabled={disabled}
              onClick={() => {
                setOpen(false);
                onManage();
              }}
            >
              Manage tags...
            </button>
          )}
          {!options.length && !onManage && (
            <button
              type="button"
              role="menuitem"
              className="account-picker-manage"
              onClick={() => setOpen(false)}
            >
              Close
            </button>
          )}
        </PopupMenu>
      )}
    </div>
  );
}
