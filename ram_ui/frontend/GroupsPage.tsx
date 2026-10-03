import { LoadingSkeleton } from "./components/LoadingSkeleton";
import { useEffect, useState } from "react";
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
import { AccountPicker } from "./components/AccountPicker";
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

function GroupIdentity({
  group,
  iconDataUrl,
}: {
  group: GroupInfo;
  iconDataUrl: string | null;
}) {
  const [iconFailed, setIconFailed] = useState(false);

  return (
    <section className="groups-card">
      <div className="groups-identity">
        <span className="groups-avatar-large">
          {iconDataUrl && !iconFailed ? (
            <img src={iconDataUrl} alt="" onError={() => setIconFailed(true)} />
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
                <img src="/icons/verification.svg" alt="" />
              </span>
            )}
          </div>
          <span className="groups-data selectable-text">Group ID: {group.id}</span>
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
        <span
          className={`groups-status-pill ${
            group.publicEntryAllowed ? "is-allowed" : "is-restricted"
          }`}
        >
          <Icon
            name={group.publicEntryAllowed ? "check" : "close"}
            tone="current-color"
          />
          {group.publicEntryAllowed
            ? "Public entry allowed"
            : "Allow join only"}
        </span>
        <span
          className={`groups-status-pill ${
            group.hasSocialModules ? "is-allowed" : "is-restricted"
          }`}
        >
          <Icon
            name={group.hasSocialModules ? "globe" : "globe-off"}
            tone="current-color"
          />
          {group.hasSocialModules ? "Social links enabled" : "No social links"}
        </span>
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
  const [isAccountsLoading, setIsAccountsLoading] = useState(true);
  const [mode, setMode] = useState<GroupMode>("name");
  const [input, setInput] = useState("");
  const [searchResults, setSearchResults] = useState<GroupSearchResult[]>([]);
  const [workspace, setWorkspace] = useState<GroupWorkspace | null>(null);
  const [pendingGroup, setPendingGroup] = useState<Pick<
    GroupSearchResult,
    "name" | "hasVerifiedBadge"
  > | null>(null);
  const [loading, setLoading] = useState(false);
  const [searching, setSearching] = useState(false);
  const [action, setAction] = useState<"join" | "leave" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);

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
      .catch(() => setAccounts([]))
      .finally(() => setIsAccountsLoading(false));
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
          ? "Enter a Roblox group name in the search field, then select Search to find matching groups."
          : "Enter a positive numeric Roblox group ID to load the group directly. A username or group name cannot be used in ID mode.",
      );
      return;
    }
    setError(null);
    setActionNotice(null);
    setWorkspace(null);
    if (mode === "id") {
      setSearchResults([]);
      const groupId = Number(value);
      if (!Number.isSafeInteger(groupId) || groupId <= 0) {
        setError("Enter a positive numeric Roblox group ID to load the group directly. A username or group name cannot be used in ID mode.");
        return;
      }
      await openGroup(groupId, {
        name: `Group ${groupId}`,
        hasVerifiedBadge: false,
      });
      return;
    }
    setSearching(true);
    try {
      setSearchResults(await searchGroups(value));
    } catch {
      setError("Roblox group search could not be completed. Check your connection and try the group name again, or search by numeric group ID.");
    } finally {
      setSearching(false);
    }
  }

  async function openGroup(
    groupId: number,
    requestedGroup?: Pick<GroupSearchResult, "name" | "hasVerifiedBadge">,
    preserveActionFeedback = false,
  ) {
    setLoading(true);
    setPendingGroup(requestedGroup ?? null);
    if (!preserveActionFeedback) {
      setError(null);
      setActionNotice(null);
    }
    try {
      setWorkspace(
        await loadGroup(
          groupId,
          selectedAccounts.map((account) => account.userId),
        ),
      );
    } catch {
      const refreshError = "The Roblox group details and account memberships could not be loaded. Check the group ID and connection, then select the group again.";
      setError((current) => preserveActionFeedback && current ? `${current} ${refreshError}` : refreshError);
    } finally {
      setLoading(false);
      setPendingGroup(null);
    }
  }

  async function updateMembership(join: boolean) {
    if (!workspace || !selectedAccounts.length) {
      setActionNotice("Select at least one managed account before requesting a Roblox group membership change.");
      return;
    }
    const userIds = selectedAccounts
      .filter((account) => membershipMap.get(account.userId)?.joined !== join)
      .map((account) => account.userId);
    if (!userIds.length) {
      setActionNotice(
        join
          ? "Every selected account is already a member of this Roblox group. No join requests were sent."
          : "None of the selected accounts belong to this Roblox group. No leave requests were sent.",
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
          `${results.length - failed.length} of ${results.length} ${join ? "join" : "leave"} requests succeeded for ${workspace.group.name}. ${failed.length} account${failed.length === 1 ? "" : "s"} could not be updated. Refresh memberships and complete any Roblox verification before retrying.`,
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
            ? `${userIds.length} selected accounts joined ${workspace.group.name} (group ${workspace.group.id}). Memberships are being refreshed.`
            : `${userIds.length} selected accounts left ${workspace.group.name} (group ${workspace.group.id}). Memberships are being refreshed.`,
        );
      }
      await openGroup(workspace.group.id, undefined, true);
    } catch (reason) {
      setError(
        reason instanceof Error
          ? reason.message
          : "The group membership request could not be completed for every selected account. Refresh memberships before retrying; some requests may already have succeeded.",
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
            <AccountPicker
              accounts={accounts}
              isLoading={isAccountsLoading}
              mode="multiple"
              open={pickerOpen}
              onOpenChange={setPickerOpen}
              selectedIds={selectedIds}
              onSelectedIdsChange={setSelectedIds}
              showManageAccounts={true}
              onManageAccounts={onNavigateAccounts}
              manageAccountsLabel="Manage on Accounts page"
            />
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
          {searching ? (
            <LoadingSkeleton layout="group-results" label="Searching groups" />
          ) : loading ? (
            <LoadingSkeleton layout="group" label={`Loading ${pendingGroup?.name ?? "group details"}`} count={selectedAccounts.length} />
          ) : !workspace && searchResults.length > 0 ? (
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
                      void openGroup(result.id, result);
                    }}
                  >
                    <span className="groups-result-heading">
                      <strong>{result.name}</strong>
                      {result.hasVerifiedBadge && (
                        <img src="/icons/verification.svg" alt="" />
                      )}
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
                <div className="groups-about">
                  {workspace.group.description ? (
                    <p className="groups-about-description">
                      {workspace.group.description}
                    </p>
                  ) : (
                    <p className="groups-about-description groups-data">
                      This group has not provided a description.
                    </p>
                  )}
                  <div className="groups-about-meta">
                    {workspace.group.communityTier !== null && (
                      <span>
                        Community tier {workspace.group.communityTier}
                      </span>
                    )}
                    {workspace.group.created && (
                      <span>Created {formatDate(workspace.group.created)}</span>
                    )}
                  </div>
                </div>
              </section>
              <section className="groups-card">
                <h2>
                  <Icon name="megaphone" />
                  Announcement
                </h2>
                {workspace.announcement ? (
                  <article className="groups-post groups-announcement">
                    <strong>{workspace.announcement.title}</strong>
                    {workspace.announcement.body && <p>{workspace.announcement.body}</p>}
                    {workspace.announcement.imageUrl && (
                      <img src={workspace.announcement.imageUrl} alt="Group announcement" />
                    )}
                    <div className="groups-announcement-meta">
                      {workspace.announcement.created && (
                        <span>{formatDate(workspace.announcement.created)}</span>
                      )}
                      {workspace.announcement.reactions.map((reaction) => (
                        <span key={reaction.label}>
                          {reaction.label} {reaction.count}
                        </span>
                      ))}
                    </div>
                  </article>
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
                  Forums
                </h2>
                {workspace.forums.length ? (
                  <div className="groups-forum-list">
                    {workspace.forums.map((forum) => (
                      <div className="groups-forum" key={forum.id}>
                        <h3>{forum.name}</h3>
                        {forum.posts.length ? forum.posts.map((post) => (
                          <article className="groups-post" key={post.id}>
                            <strong>{post.title}</strong>
                            {post.body && <p>{post.body}</p>}
                            <span>
                              {post.author || "Unknown author"}
                              {post.created ? ` · ${formatDate(post.created)}` : ""}
                              {` · ${post.commentCount} comments`}
                            </span>
                          </article>
                        )) : <p className="groups-forum-empty">No posts in this forum.</p>}
                      </div>
                    ))}
                  </div>
                ) : (
                  <EmptyCard
                    icon="message-square"
                    title="Forums unavailable"
                    message={workspace.forumStatus || "This group has no forums."}
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
