import type { Friend, FriendStatus } from "./ipc";
export type FriendFilter = "all" | FriendStatus | "requests";
export type FriendSort = "status" | "nameAsc" | "nameDesc" | "userAsc" | "userDesc";
export const friendStatusLabels: Record<FriendStatus, string> = {playing: "Playing", studio: "In Studio", online: "Online", offline: "Offline", unknown: "Unavailable"};
const rank: Record<FriendStatus, number> = {playing: 0, studio: 1, online: 2, offline: 3, unknown: 4};
export function friendCounts(friends: readonly Friend[]) {
  const counts = {all: friends.length, playing: 0, studio: 0, online: 0, offline: 0, unknown: 0};
  for (const friend of friends) counts[friend.status] += 1;
  return counts;
}
export function visibleFriends(friends: readonly Friend[], search: string, filter: FriendFilter, sort: FriendSort): Friend[] {
  const query = search.trim().toLocaleLowerCase();
  return friends.filter(friend => (filter === "all" || friend.status === filter) && `${friend.displayName} ${friend.username}`.toLocaleLowerCase().includes(query)).sort((a,b) => {
    const name = (a.displayName || a.username).localeCompare(b.displayName || b.username, undefined, {numeric: true, sensitivity: "base"}) || a.userId - b.userId;
    const username = a.username.localeCompare(b.username, undefined, {numeric: true, sensitivity: "base"}) || a.userId - b.userId;
    if (sort === "nameAsc") return name;
    if (sort === "nameDesc") return -name;
    if (sort === "userAsc") return username;
    if (sort === "userDesc") return -username;
    return rank[a.status] - rank[b.status] || name;
  });
}
export function reconcileFriendsSelection(selected: ReadonlySet<number>, friends: readonly {userId: number}[]): Set<number> {
  const ids = new Set(friends.map(friend => friend.userId));
  return new Set([...selected].filter(id => ids.has(id)));
}
export function canJoinFriend(friend: Friend): boolean { return friend.status === "playing"; }
export function friendLabel(friend: {userId:number; displayName:string; username:string}): string { return friend.displayName || friend.username || `User ${friend.userId}`; }
