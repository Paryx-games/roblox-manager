import { useEffect, useRef } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import type { AccountSummary } from "../lib/ipc";
import { AccountAvatar } from "./AccountAvatar";
import { Icon } from "./Icon";

type AccountPickerProps = {
  accounts: AccountSummary[];
  mode: "multiple" | "single";
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedIds?: Set<number>;
  selectedId?: number;
  onSelectedIdsChange?: (ids: Set<number>) => void;
  onSelectedIdChange?: (id: number | undefined) => void;
  emptyLabel?: string;
  noAccountLabel?: string;
  unselectedLabel?: string;
  manageLabel?: string;
  onManage?: () => void;
  className?: string;
  avatarClassName?: string;
};

export function AccountPicker({
  accounts,
  mode,
  open,
  onOpenChange,
  selectedIds = new Set<number>(),
  selectedId,
  onSelectedIdsChange,
  onSelectedIdChange,
  emptyLabel = "No accounts available",
  noAccountLabel = "No account",
  unselectedLabel = "Select account",
  manageLabel,
  onManage,
  className = "account-picker",
  avatarClassName = "account-picker-avatar",
}: AccountPickerProps) {
  const pickerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);

  const selectedAccounts = accounts.filter((account) =>
    mode === "multiple"
      ? selectedIds.has(account.userId)
      : account.userId === selectedId,
  );

  useEffect(() => {
    function closeOnOutsidePointer(event: PointerEvent) {
      if (
        pickerRef.current &&
        !pickerRef.current.contains(event.target as Node)
      ) {
        onOpenChange(false);
      }
    }
    document.addEventListener("pointerdown", closeOnOutsidePointer);
    return () =>
      document.removeEventListener("pointerdown", closeOnOutsidePointer);
  }, [onOpenChange]);

  useEffect(() => {
    if (!open) return;
    optionRefs.current[0]?.focus();
    function closeOnEscape(event: globalThis.KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        onOpenChange(false);
        triggerRef.current?.focus();
      }
    }
    document.addEventListener("keydown", closeOnEscape);
    return () => document.removeEventListener("keydown", closeOnEscape);
  }, [onOpenChange, open]);

  function selectAccount(id: number | undefined) {
    if (mode === "multiple") {
      const next = new Set(selectedIds);
      if (id === undefined) return;
      if (next.has(id)) next.delete(id);
      else next.add(id);
      onSelectedIdsChange?.(next);
      return;
    }
    onSelectedIdChange?.(id);
    onOpenChange(false);
    triggerRef.current?.focus();
  }

  function handleMenuKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
    if (!optionRefs.current.length) return;
    const currentIndex = optionRefs.current.findIndex(
      (option) => option === document.activeElement,
    );
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const direction = event.key === "ArrowDown" ? 1 : -1;
      const nextIndex =
        (currentIndex + direction + optionRefs.current.length) %
        optionRefs.current.length;
      optionRefs.current[nextIndex]?.focus();
    } else if (event.key === "Home") {
      event.preventDefault();
      optionRefs.current[0]?.focus();
    } else if (event.key === "End") {
      event.preventDefault();
      optionRefs.current[optionRefs.current.length - 1]?.focus();
    }
  }

  return (
    <div
      className={`${className} account-picker account-picker-${mode} ${open ? "is-open" : ""}`}
      ref={pickerRef}
    >
      <button
        className="account-picker-trigger"
        ref={triggerRef}
        type="button"
        aria-expanded={open}
        aria-haspopup="listbox"
        onClick={() => onOpenChange(!open)}
      >
        <span className="account-picker-left">
          {mode === "multiple" && selectedAccounts.length > 0 && (
            <span className="account-picker-facepile">
              {selectedAccounts.slice(0, 3).map((account) => (
                <AccountAvatar
                  key={account.userId}
                  account={account}
                  className={avatarClassName}
                />
              ))}
              {selectedAccounts.length > 3 && (
                <span className="account-picker-overflow">
                  +{selectedAccounts.length - 3}
                </span>
              )}
            </span>
          )}
          {mode === "single" && selectedAccounts[0] ? (
            <AccountAvatar
              account={selectedAccounts[0]}
              className={avatarClassName}
            />
          ) : null}
          <span>
            {mode === "multiple"
              ? selectedAccounts.length
                ? selectedAccounts.map((account) => account.username).join(", ")
                : "No accounts selected"
              : selectedAccounts[0]?.username ?? unselectedLabel}
          </span>
        </span>
        <Icon name="chevron-down" />
      </button>
      {open && (
        <div
          className="account-picker-menu"
          role="listbox"
          aria-label="Account selection"
          onKeyDown={handleMenuKeyDown}
        >
          {accounts.map((account, index) => {
            const selected =
              mode === "multiple"
                ? selectedIds.has(account.userId)
                : account.userId === selectedId;
            return (
              <button
                className={`account-picker-option ${selected ? "is-selected" : ""}`}
                key={account.userId}
                ref={(element) => {
                  optionRefs.current[index] = element;
                }}
                type="button"
                role="option"
                aria-selected={selected}
                onClick={() => selectAccount(account.userId)}
              >
                {mode === "multiple" && (
                  <span className="account-picker-checkbox">
                    {selected && <Icon name="check" />}
                  </span>
                )}
                <AccountAvatar account={account} className={avatarClassName} />
                <span>{account.username}</span>
              </button>
            );
          })}
          {!accounts.length && (
            <span className="account-picker-empty">{emptyLabel}</span>
          )}
          {mode === "single" && (
            <button
              className={`account-picker-option ${selectedId === undefined ? "is-selected" : ""}`}
              ref={(element) => {
                optionRefs.current[accounts.length] = element;
              }}
              type="button"
              role="option"
              aria-selected={selectedId === undefined}
              onClick={() => selectAccount(undefined)}
            >
              <span className="account-picker-avatar">--</span>
              <span>{noAccountLabel}</span>
            </button>
          )}
          {manageLabel && onManage && (
            <>
              <div className="account-picker-divider" />
              <button
                className="account-picker-manage"
                type="button"
                onClick={() => {
                  onManage();
                  onOpenChange(false);
                }}
              >
                <Icon name="id-card" />
                {manageLabel}
              </button>
            </>
          )}
        </div>
      )}
    </div>
  );
}