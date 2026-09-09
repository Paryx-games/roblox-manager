import { useEffect, useRef, useState } from "react";
import {
  changeGroupMembership,
  listAccounts,
  loadGroup,
  openGroupChallenge,
  searchGroups,
  type AccountSummary,
  type GroupInfo,
  type GroupMembership,
  type GroupSearchResult,
  type GroupWorkspace,
} from "./lib/ipc";
import { AccountAvatar } from "./components/AccountAvatar";
import { Icon } from "./components/Icon";

type GroupMode = "name" | "id";

type GroupsPageProps = {
  selectedIds: Set<number>;
  setSelectedIds: (ids: Set<number>) => void;
  onNavigateAccounts: () => void;
};

function formatDate(value: string | null) {
  if (!value) return "";
  const date = new Date(value);
  return Number.isNaN(date.valueOf()) ? value : date.toLocaleDateString();
}

function posterLabel(value: { displayName: string; username: string } | null) {
  return value?.displayName || value?.username || "Unknown author";
}

function GroupIdentity({
  group,
  iconDataUrl,
}: {
  group: GroupInfo;
  iconDataUrl: string | null;
}) {
  return (
    <section className="groups-card">
      <div className="groups-identity">
        <span className="groups-avatar-large">
          {iconDataUrl ? (
            <img src={iconDataUrl} alt="" />
          ) : (
            group.name.slice(0, 1).toUpperCase()
          )}
        </span>
        <div>
          <div className="groups-name-row">
            <strong>{group.name}</strong>
            {group.hasVerifiedBadge && (
              <span
                className="groups-verified"
                data-tip="Verified group"
                aria-label="Verified group"
              >
                <Icon name="shield-check" />
              </span>
            )}
          </div>
          <span className="groups-data">Group ID: {group.id}</span>
        </div>
      </div>
      <div className="groups-chip-row">
        <span className="groups-stat-chip">
          <Icon name="groups" />
          <b>{group.memberCount.toLocaleString()}</b> members
        </span>
        {group.owner && (
          <span className="groups-stat-chip">
            <Icon name="crown" />
            Owned by <b>{group.owner.displayName || group.owner.username}</b>
          </span>
        )}
      </div>
      <div className="groups-chip-row">
        <span className="groups-status-pill">
          <Icon name="link" />
          {group.publicEntryAllowed
            ? "Public entry allowed"
            : "Public entry restricted"}
        </span>
        {group.hasSocialModules && (
          <span className="groups-status-pill">
            <Icon name="link" />
            Social links enabled
          </span>
        )}
      </div>
    </section>
  );
}

function EmptyCard({
  icon,
  title,
  message,
}: {
  icon: string;
  title: string;
  message: string;
}) {
  return (
    <div className="groups-empty-inline">
      <Icon name={icon} />
      <div className="groups-empty-title">{title}</div>
      <div>{message}</div>
    </div>
  );
}

function MembershipStatus({
  membership,
  loading,
}: {
  membership?: GroupMembership;
  loading: boolean;
}) {
  if (loading) return <span className="groups-data">Checking...</span>;
  if (!membership)
    return <span className="groups-data">No membership data</span>;
  if (!membership.joined)
    return <span className="groups-data">Not a member</span>;
  return (
    <span className="groups-status-text">
      {membership.roleName || "Member"} · rank {membership.roleRank}
    </span>
  );
}

export function GroupsPage({
  selectedIds,
  setSelectedIds,
  onNavigateAccounts,
}: GroupsPageProps) {
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [mode, setMode] = useState<GroupMode>("name");
  const [input, setInput] = useState("");
  const [searchResults, setSearchResults] = useState<GroupSearchResult[]>([]);
  const [workspace, setWorkspace] = useState<GroupWorkspace | null>(null);
  const [loading, setLoading] = useState(false);
  const [searching, setSearching] = useState(false);
  const [action, setAction] = useState<"join" | "leave" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const pickerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void listAccounts()
      .then((items) => {
        setAccounts(items);
        setSelectedIds(
          new Set(
            [...selectedIds].filter((id) =>
              items.some((account) => account.userId === id),
            ),
          ),
        );
      })
      .catch(() => setAccounts([]));
  }, []);

  useEffect(() => {
    function closePicker(event: MouseEvent) {
      if (
        pickerRef.current &&
        !pickerRef.current.contains(event.target as Node)
      )
        setPickerOpen(false);
    }
    document.addEventListener("mousedown", closePicker);
    return () => document.removeEventListener("mousedown", closePicker);
  }, []);

  const selectedAccounts = accounts.filter((account) =>
    selectedIds.has(account.userId),
  );
  const membershipMap = new Map(
    (workspace?.memberships ?? []).map((membership) => [
      membership.userId,
      membership,
    ]),
  );
  const canJoin = Boolean(
    workspace &&
    selectedAccounts.some(
      (account) => !membershipMap.get(account.userId)?.joined,
    ),
  );
  const canLeave = Boolean(
    workspace &&
    selectedAccounts.some(
      (account) => membershipMap.get(account.userId)?.joined,
    ),
  );

  function changeMode(nextMode: GroupMode) {
    setMode(nextMode);
    setInput("");
    setSearchResults([]);
    setError(null);
  }

  async function submitSearch() {
    const value = input.trim();
    if (!value) {
      setError(
        mode === "name"
          ? "Enter a group name to search."
          : "Enter a numeric Roblox group ID.",
      );
      return;
    }
    setError(null);
    setActionNotice(null);
    if (mode === "id") {
      setSearchResults([]);
      const groupId = Number(value);
      if (!Number.isSafeInteger(groupId) || groupId <= 0) {
        setError("Enter a numeric Roblox group ID.");
        return;
      }
      await openGroup(groupId);
      return;
    }
    setSearching(true);
    try {
      setSearchResults(await searchGroups(value));
    } catch {
      setError("Roblox group search failed.");
    } finally {
      setSearching(false);
    }
  }

  async function openGroup(groupId: number) {
    setLoading(true);
    setError(null);
    setActionNotice(null);
    try {
      setWorkspace(
        await loadGroup(
          groupId,
          selectedAccounts.map((account) => account.userId),
        ),
      );
    } catch {
      setError("Roblox group could not be loaded.");
    } finally {
      setLoading(false);
    }
  }

  async function updateMembership(join: boolean) {
    if (!workspace || !selectedAccounts.length) {
      setActionNotice("Select at least one account first.");
      return;
    }
    const userIds = selectedAccounts
      .filter((account) => membershipMap.get(account.userId)?.joined !== join)
      .map((account) => account.userId);
    if (!userIds.length) {
      setActionNotice(
        join
          ? "All selected accounts are already in the group."
          : "None of the selected accounts are in the group.",
      );
      return;
    }
    setAction(join ? "join" : "leave");
    setError(null);
    setActionNotice(null);
    try {
      const results = await changeGroupMembership(
        workspace.group.id,
        join,
        userIds,
      );
      const failed = results.filter((result) => !result.ok);
      if (failed.length) {
        setError(
          `${failed.length} account${failed.length === 1 ? "" : "s"} could not be updated.`,
        );
        if (failed[0]?.challenge) {
          const account = selectedAccounts.find(
            (candidate) => candidate.userId === failed[0].userId,
          );
          if (account)
            await openGroupChallenge(workspace.group.id, account.userId);
        }
      } else {
        setActionNotice(
          join
            ? "Selected accounts joined the group."
            : "Selected accounts left the group.",
        );
      }
      await openGroup(workspace.group.id);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "Group membership could not be changed.",
      );
    } finally {
      setAction(null);
    }
  }

  return (
    <>
      <div className="header-row">
        <h1 className="header-title">Groups</h1>
      </div>
      <main className="groups-page">
        <aside className="groups-rail">
          <section className="groups-card">
            <h2>Find a group</h2>
            <p className="groups-subtitle">
              Search Roblox groups, then manage membership for the selected
              accounts.
            </p>
            <p className="groups-banner" role="status">
              <Icon name="warning" />
              <span>
                <strong>Verification may be required.</strong> RM will open a
                signed-in group page when Roblox presents a challenge.
              </span>
            </p>
            <div
              className="groups-mode-toggle"
              role="tablist"
              aria-label="Group lookup mode"
            >
              <button
                className={mode === "name" ? "is-active" : ""}
                type="button"
                role="tab"
                aria-selected={mode === "name"}
                onClick={() => changeMode("name")}
              >
                By name
              </button>
              <button
                className={mode === "id" ? "is-active" : ""}
                type="button"
                role="tab"
                aria-selected={mode === "id"}
                onClick={() => changeMode("id")}
              >
                By ID
              </button>
            </div>
            <label className="groups-input-wrap">
              <Icon name={mode === "name" ? "search" : "hash"} />
              <input
                value={input}
                onChange={(event) => setInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void submitSearch();
                }}
                placeholder={
                  mode === "name" ? "Search by group name" : "Enter a group ID"
                }
                aria-label={
                  mode === "name" ? "Search by group name" : "Enter a group ID"
                }
              />
            </label>
            <button
              className="groups-button primary"
              type="button"
              disabled={searching || loading}
              onClick={() => void submitSearch()}
            >
              <Icon name={mode === "name" ? "search" : "hash"} />
              {searching
                ? "Searching..."
                : loading
                  ? "Loading..."
                  : mode === "name"
                    ? "Search groups"
                    : "Load group"}
            </button>
            {error && (
              <p className="groups-error" role="alert">
                {error}
              </p>
            )}
            {actionNotice && (
              <p className="groups-notice" role="status">
                {actionNotice}
              </p>
            )}
          </section>

          <section className="groups-card">
            <div className="groups-card-head">
              <h2>Selected accounts</h2>
              <span className="groups-count">
                {selectedAccounts.length} selected
              </span>
            </div>
            <div
              className={`groups-picker ${pickerOpen ? "is-open" : ""}`}
              ref={pickerRef}
            >
              <button
                className="groups-picker-trigger"
                type="button"
                aria-expanded={pickerOpen}
                onClick={() => setPickerOpen((open) => !open)}
              >
                <span className="groups-picker-left">
                  {selectedAccounts.length > 0 && (
                    <span className="groups-facepile">
                      {selectedAccounts.slice(0, 3).map((account) => (
                        <AccountAvatar
                          key={account.userId}
                          account={account}
                          className="groups-avatar-small"
                        />
                      ))}
                      {selectedAccounts.length > 3 && (
                        <span className="groups-avatar-overflow">
                          +{selectedAccounts.length - 3}
                        </span>
                      )}
                    </span>
                  )}
                  <span>
                    {selectedAccounts.length
                      ? selectedAccounts
                          .map((account) => account.username)
                          .join(", ")
                      : "No accounts selected"}
                  </span>
                </span>
                <Icon name="chevron-down" />
              </button>
              {pickerOpen && (
                <div className="groups-picker-menu">
                  {accounts.map((account) => {
                    const checked = selectedIds.has(account.userId);
                    return (
                      <button
                        className={`groups-picker-option ${checked ? "is-checked" : ""}`}
                        key={account.userId}
                        type="button"
                        onClick={() => {
                          const next = new Set(selectedIds);
                          if (checked) next.delete(account.userId);
                          else next.add(account.userId);
                          setSelectedIds(next);
                        }}
                      >
                        <span className="groups-checkbox">
                          {checked && <Icon name="check" />}
                        </span>
                        <AccountAvatar
                          account={account}
                          className="groups-avatar-small"
                        />
                        <span>{account.username}</span>
                      </button>
                    );
                  })}
                  <div className="groups-picker-divider" />
                  <button
                    className="groups-picker-manage"
                    type="button"
                    onClick={onNavigateAccounts}
                  >
                    <Icon name="id-card" />
                    Manage on Accounts page
                  </button>
                </div>
              )}
            </div>
            <div className="groups-action-row">
              <button
                className="groups-button primary"
                type="button"
                disabled={!canJoin || action !== null}
                onClick={() => void updateMembership(true)}
              >
                <Icon name="log-in" />
                {action === "join" ? "Joining..." : "Join selected"}
              </button>
              <button
                className="groups-button"
                type="button"
                disabled={!canLeave || action !== null}
                onClick={() => void updateMembership(false)}
              >
                <Icon name="log-out" />
                {action === "leave" ? "Leaving..." : "Leave selected"}
              </button>
            </div>
          </section>
        </aside>

        <section className="groups-detail">
          {!workspace && searchResults.length > 0 ? (
            <section className="groups-results-page">
              <div className="groups-results-header">
                <div>
                  <h2>Search results</h2>
                  <p>
                    {searchResults.length} groups found for &quot;{input}&quot;.
                  </p>
                </div>
                <span className="groups-count">
                  {searchResults.length} results
                </span>
              </div>
              <div className="groups-results" aria-label="Group search results">
                {searchResults.map((result) => (
                  <button
                    className="groups-result"
                    key={result.id}
                    type="button"
                    onClick={() => {
                      setInput(String(result.id));
                      void openGroup(result.id);
                    }}
                  >
                    <span className="groups-result-heading">
                      <strong>{result.name}</strong>
                      {result.hasVerifiedBadge && <Icon name="shield-check" />}
                    </span>
                    <span>
                      {result.memberCount.toLocaleString()} members
                      {result.hasVerifiedBadge ? " · Verified" : ""}
                    </span>
                    {result.description && <small>{result.description}</small>}
                    <span className="groups-result-action">
                      View group <Icon name="chevron-down" />
                    </span>
                  </button>
                ))}
              </div>
            </section>
          ) : !workspace ? (
            <section className="groups-card groups-start">
              <Icon name="search" />
              <strong>Find a group to get started</strong>
              <span>
                Search by name or enter a group ID. Details and selected-account
                membership will appear here.
              </span>
            </section>
          ) : (
            <>
              {searchResults.length > 0 && (
                <button
                  className="groups-back-button"
                  type="button"
                  onClick={() => {
                    setWorkspace(null);
                    setError(null);
                    setActionNotice(null);
                  }}
                >
                  <Icon name="chevron-down" />
                  Back to search results
                </button>
              )}
              <GroupIdentity
                group={workspace.group}
                iconDataUrl={workspace.iconDataUrl}
              />
              <section className="groups-card">
                <h2>About</h2>
                <div className="groups-copy">
                  {workspace.group.description ? (
                    <p>{workspace.group.description}</p>
                  ) : (
                    <p className="groups-data">
                      This group has not provided a description.
                    </p>
                  )}
                  {workspace.group.communityTier !== null && (
                    <p className="groups-data">
                      Community tier {workspace.group.communityTier}
                      {workspace.group.created
                        ? ` · Created ${formatDate(workspace.group.created)}`
                        : ""}
                    </p>
                  )}
                </div>
              </section>
              <section className="groups-card">
                <h2>
                  <Icon name="megaphone" />
                  Announcement
                </h2>
                {workspace.group.shout ? (
                  <div className="groups-post">
                    <p>{workspace.group.shout.body}</p>
                    <span>
                      {posterLabel(workspace.group.shout.poster)}
                      {workspace.group.shout.created
                        ? ` · ${formatDate(workspace.group.shout.created)}`
                        : ""}
                    </span>
                  </div>
                ) : (
                  <EmptyCard
                    icon="megaphone"
                    title="No announcement posted"
                    message="This group has not posted an announcement yet."
                  />
                )}
              </section>
              <section className="groups-card">
                <h2>
                  <Icon name="message-square" />
                  Recent wall posts
                </h2>
                {workspace.announcements.length ? (
                  <div className="groups-post-list">
                    {workspace.announcements.map((post) => (
                      <article className="groups-post" key={post.id}>
                        <p>{post.body}</p>
                        <span>
                          {posterLabel(post.poster)}
                          {post.created ? ` · ${formatDate(post.created)}` : ""}
                        </span>
                      </article>
                    ))}
                  </div>
                ) : (
                  <EmptyCard
                    icon="message-square"
                    title="Wall posts unavailable"
                    message="Roblox's current public group API does not expose wall posts."
                  />
                )}
              </section>
              <section className="groups-card">
                <div className="groups-card-head">
                  <h2>Selected account membership</h2>
                  <span className="groups-count">
                    {selectedAccounts.length}
                  </span>
                </div>
                {selectedAccounts.length ? (
                  <div className="groups-membership-list">
                    {selectedAccounts.map((account) => (
                      <div className="groups-membership" key={account.userId}>
                        <AccountAvatar
                          account={account}
                          className="groups-avatar-small"
                        />
                        <strong>{account.label}</strong>
                        <MembershipStatus
                          membership={membershipMap.get(account.userId)}
                          loading={loading}
                        />
                      </div>
                    ))}
                  </div>
                ) : (
                  <p className="groups-data">
                    Select accounts to inspect membership.
                  </p>
                )}
              </section>
            </>
          )}
        </section>
      </main>
    </>
  );
}
