import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { AccountAvatar, type AccountIdentity } from "./AccountAvatar";
import { allAccountsSelected, toggleAllAccounts } from "../lib/accountSelection";
import { Icon } from "./Icon";
import { PopupMenu } from "./PopupMenu";

type AccountPickerProps = {
  accounts: AccountIdentity[];
  mode: "multiple" | "single" | "groups";
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedIds?: Set<number>;
  selectedId?: number;
  onSelectedIdsChange?: (ids: Set<number>) => void;
  onSelectedIdChange?: (id: number | undefined) => void;
  emptyLabel?: string;
  noAccountLabel?: string;
  unselectedLabel?: string;
  showManageAccounts?: boolean;
  onManageAccounts?: () => void;
  manageAccountsLabel?: string;
  groups?: Array<{ name: string; color: string }>;
  selectedGroup?: string;
  onSelectedGroupChange?: (group: string) => void;
  createGroupLabel?: string;
  onCreateGroup?: () => void;
  deleteGroupLabel?: string;
  onDeleteGroup?: () => void;
  disabled?: boolean;
  isLoading?: boolean;
  className?: string;
  avatarClassName?: string;
  showAllAccounts?: boolean;
  disabledAccountIds?: ReadonlySet<number>;
  ariaLabel?: string;
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
  showManageAccounts = false,
  onManageAccounts,
  manageAccountsLabel = "Manage accounts",
  groups = [],
  selectedGroup = "",
  onSelectedGroupChange,
  createGroupLabel,
  onCreateGroup,
  deleteGroupLabel,
  onDeleteGroup,
  disabled = false,
  isLoading = false,
  className = "account-picker",
  avatarClassName = "account-picker-avatar",
  showAllAccounts = false,
  disabledAccountIds,
  ariaLabel,
}: AccountPickerProps) {
  const pickerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [isMenuAbove, setIsMenuAbove] = useState(false);
  const eligibleIds = accounts.filter(account => !disabledAccountIds?.has(account.userId)).map(account => account.userId);
  const hasAllOption = mode === "multiple" && showAllAccounts;
  const allSelected = hasAllOption && allAccountsSelected(selectedIds, eligibleIds);

  useLayoutEffect(() => {
    if (!open) return;
    const updatePlacement = () => {
      const bounds = triggerRef.current?.getBoundingClientRect();
      if (!bounds) return;
      const menuHeight = Math.min(240, window.innerHeight - 16);
      setIsMenuAbove(
        bounds.bottom + 4 + menuHeight > window.innerHeight &&
          bounds.top - 4 > menuHeight,
      );
    };
    updatePlacement();
    window.addEventListener("resize", updatePlacement);
    window.addEventListener("scroll", updatePlacement, true);
    return () => {
      window.removeEventListener("resize", updatePlacement);
      window.removeEventListener("scroll", updatePlacement, true);
    };
  }, [open]);

  const selectedAccounts = accounts.filter((account) =>
    mode === "multiple"
      ? selectedIds.has(account.userId)
      : account.userId === selectedId,
  );
  const selectedGroupOption = groups.find(
    (group) => group.name === selectedGroup,
  );

  useEffect(() => {
    if (!open) return;
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
  }, [onOpenChange, open]);

  useEffect(() => {
    if (!open) return;
    optionRefs.current[0]?.focus();
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

  function selectGroup(group: string) {
    onSelectedGroupChange?.(group);
    onOpenChange(false);
    triggerRef.current?.focus();
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
        disabled={disabled || isLoading}
        aria-busy={isLoading}
        aria-label={isLoading ? "Loading accounts" : ariaLabel}
        aria-expanded={open}
        aria-haspopup="menu"
        onClick={() => onOpenChange(!open)}
      >
        <span className="account-picker-left">
          {isLoading ? <span className="loading-skeleton-block loading-skeleton-picker" /> : <>
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
          {mode === "groups" && (
            <span
              className="account-picker-group-marker"
              style={
                {
                  "--picker-group-color":
                    selectedGroupOption?.color ?? "var(--text-muted)",
                } as CSSProperties
              }
              aria-hidden="true"
            />
          )}
          <span
            className={
              mode === "multiple" && selectedAccounts.length === 0
                ? "account-picker-empty-selection"
                : undefined
            }
          >
            {mode === "groups"
              ? selectedGroup || "Ungrouped"
              : mode === "multiple"
                ? allSelected
                  ? "All accounts"
                  : selectedAccounts.length
                  ? selectedAccounts
                      .map((account) => account.username)
                      .join(", ")
                  : "No accounts selected"
                : (selectedAccounts[0]?.username ?? unselectedLabel)}
          </span>
          </>}
        </span>
        <Icon name="chevron-down" />
      </button>
      {open && (
        <PopupMenu
          className={`account-picker-menu ${isMenuAbove ? "is-above" : ""}`}
          onClose={() => onOpenChange(false)}
        >
          {hasAllOption && <>
            <button className={`account-picker-option ${allSelected ? "is-selected" : ""}`} ref={element => { optionRefs.current[0] = element; }} type="button" role="menuitemcheckbox" aria-checked={allSelected} disabled={!eligibleIds.length} onClick={() => onSelectedIdsChange?.(toggleAllAccounts(selectedIds, eligibleIds))}>
              <span className="account-picker-checkbox">{allSelected && <Icon name="check" />}</span>
              <Icon name="accounts" />
              <span>All accounts</span>
            </button>
            <div className="account-picker-divider" />
          </>}
          {mode !== "groups" &&
            accounts.map((account, index) => {
              const selected =
                mode === "multiple"
                  ? selectedIds.has(account.userId)
                  : account.userId === selectedId;
              return (
                <button
                  className={`account-picker-option ${selected ? "is-selected" : ""}`}
                  key={account.userId}
                  ref={(element) => {
                    optionRefs.current[index + (hasAllOption ? 1 : 0)] = element;
                  }}
                  type="button"
                  role={mode === "multiple" ? "menuitemcheckbox" : "menuitemradio"}
                  aria-checked={selected}
                  disabled={disabledAccountIds?.has(account.userId)}
                  onClick={() => selectAccount(account.userId)}
                >
                  {mode === "multiple" && (
                    <span className="account-picker-checkbox">
                      {selected && <Icon name="check" />}
                    </span>
                  )}
                  <AccountAvatar
                    account={account}
                    className={avatarClassName}
                  />
                  <span>{account.username}</span>
                </button>
              );
            })}
          {mode !== "groups" && !accounts.length && (
            <span className="account-picker-empty">{emptyLabel}</span>
          )}
          {mode === "single" && (
            <button
              className={`account-picker-option ${selectedId === undefined ? "is-selected" : ""}`}
              ref={(element) => {
                optionRefs.current[accounts.length] = element;
              }}
              type="button"
              role="menuitemradio"
              aria-checked={selectedId === undefined}
              onClick={() => selectAccount(undefined)}
            >
              <span className="account-picker-avatar">--</span>
              <span>{noAccountLabel}</span>
            </button>
          )}
          {mode === "groups" && (
            <>
              <button
                className={`account-picker-option ${selectedGroup === "" ? "is-selected" : ""}`}
                ref={(element) => {
                  optionRefs.current[0] = element;
                }}
                type="button"
                role="menuitemradio"
                aria-checked={selectedGroup === ""}
                onClick={() => selectGroup("")}
              >
                <span
                  className="account-picker-group-marker"
                  style={
                    {
                      "--picker-group-color": "var(--text-muted)",
                    } as CSSProperties
                  }
                  aria-hidden="true"
                />
                <span>Ungrouped</span>
              </button>
              {groups.map((group, index) => (
                <button
                  className={`account-picker-option ${selectedGroup === group.name ? "is-selected" : ""}`}
                  key={group.name}
                  ref={(element) => {
                    optionRefs.current[index + 1] = element;
                  }}
                  type="button"
                  role="menuitemradio"
                  aria-checked={selectedGroup === group.name}
                  onClick={() => selectGroup(group.name)}
                >
                  <span
                    className="account-picker-group-marker"
                    style={
                      { "--picker-group-color": group.color } as CSSProperties
                    }
                    aria-hidden="true"
                  />
                  <span>{group.name}</span>
                </button>
              ))}
            </>
          )}
          {mode === "groups" && createGroupLabel && onCreateGroup && (
            <>
              <div className="account-picker-divider" />
              <button
                className="account-picker-manage"
                type="button"
                disabled={disabled}
                onClick={() => {
                  onCreateGroup();
                  onOpenChange(false);
                }}
              >
                <Icon name="add" />
                {createGroupLabel}
              </button>
            </>
          )}
          {mode === "groups" &&
            deleteGroupLabel &&
            onDeleteGroup &&
            selectedGroup && (
              <>
                <div className="account-picker-divider" />
                <button
                  className="account-picker-manage account-picker-danger"
                  type="button"
                  disabled={disabled}
                  onClick={() => {
                    onDeleteGroup();
                    onOpenChange(false);
                  }}
                >
                  <Icon name="delete-danger" />
                  {deleteGroupLabel}
                </button>
              </>
            )}
          {showManageAccounts && onManageAccounts && (
            <>
              <div className="account-picker-divider" />
              <button
                className="account-picker-manage"
                type="button"
                onClick={() => {
                  onManageAccounts();
                  onOpenChange(false);
                }}
              >
                <Icon name="id-card" />
                {manageAccountsLabel}
              </button>
            </>
          )}
        </PopupMenu>
      )}
    </div>
  );
}
