import { useState } from "react";
import { AccountAvatar } from "./AccountAvatar";
import { Icon } from "./Icon";
import { FriendContextMenu } from "./FriendContextMenu";
import { canJoinFriend, friendLabel, friendStatusLabels } from "../lib/friends";
import type { Friend, FriendUser } from "../lib/ipc";

export function FriendIdentity({ user }: {user: FriendUser}) {
  return <span className="friend-identity"><AccountAvatar account={{...user, label: friendLabel(user)}} /><span><strong>{friendLabel(user)}</strong><small>{user.username ? `@${user.username}` : `User ID ${user.userId}`}</small></span></span>;
}
export function FriendPresence({ friend }: {friend: Friend}) {
  return <span className={`friend-presence friend-presence-${friend.status}`}><span aria-hidden="true" />{friendStatusLabels[friend.status]}</span>;
}
export function FriendGameInfo({friend}: {friend: Friend}) {
  const inGame = friend.status === "playing";
  const studio = friend.status === "studio";
  const [failedIcon, setFailedIcon] = useState<string | null>(null);
  const gameIcon = inGame && friend.gameIcon && friend.gameIcon !== failedIcon ? friend.gameIcon : null;
  return <span className="friend-game">{(inGame || studio || friend.status === "online") && <span className="friend-game-icon">{gameIcon ? <img src={gameIcon} alt="" loading="lazy" decoding="async" onError={() => setFailedIcon(gameIcon)} /> : <Icon name={studio ? "roblox-studio" : "roblox"} />}</span>}<span><strong>{studio ? "Roblox Studio" : inGame ? friend.gameName || friend.placeName || friend.activity || "In game" : friendStatusLabels[friend.status]}</strong><small>{friend.placeName || (friend.placeId ? `Place ${friend.placeId}` : friend.activity || (inGame ? "Game details unavailable" : studio ? "Editing details unavailable" : ""))}</small></span></span>;
}
export function FriendId({value, label, onCopy}: {value: string | number | null; label: string; onCopy: (value:string, label:string)=>void}) {
  return <span className="friend-id">{value !== null ? <><code data-tip={String(value)}>{value}</code><button type="button" className="icon-button" aria-label={`Copy ${label}`} data-tip={`Copy ${label}`} onClick={() => onCopy(String(value), label)}><Icon name="copy" /></button></> : <span className="friends-muted">Unavailable</span>}</span>;
}
export function FriendRow({friend, checked, selected, busy, multiInstance, onCheck, onSelect, onCopy, onJoin, onManage, onProfile}: {
  friend: Friend; checked: boolean; selected: boolean; busy: boolean; multiInstance: boolean;
  onCheck: () => void; onSelect: () => void; onCopy:(value:string,label:string)=>void;
  onJoin:(friend:Friend, expectedJobId?:string,requireNewInstance?:boolean)=>void; onManage:(friend:Friend, action:"remove"|"block")=>void; onProfile:(userId:number)=>void;
}) {
  return <tr data-selected={selected} onClick={event => { if (!(event.target instanceof Element) || !event.target.closest("button,input,a")) onSelect(); }}>
    <td><input type="checkbox" checked={checked} aria-label={`Select ${friendLabel(friend)}`} onChange={onCheck} /></td>
    <td><button className="friend-name-button" type="button" aria-pressed={selected} onClick={onSelect}><FriendIdentity user={friend} /></button></td>
    <td><FriendPresence friend={friend} /></td>
    <td><FriendGameInfo friend={friend} /></td>
    <td><FriendId value={friend.jobId} label={`Job ID for ${friendLabel(friend)}`} onCopy={onCopy} /></td>
    <td><div className="friend-row-actions">{canJoinFriend(friend) && <button className="account-button primary" type="button" disabled={busy} onClick={() => onJoin(friend)}>Join</button>}<FriendContextMenu friend={friend} busy={busy} multiInstance={multiInstance} onCopy={onCopy} onJoin={onJoin} onManage={onManage} onView={onSelect} onProfile={onProfile} /></div></td>
  </tr>;
}
