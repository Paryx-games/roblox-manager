import { useEffect, useState } from "react";
import { getFriendDetails, operationError, type Friend, type FriendDetails, type FriendDetailTab } from "../lib/ipc";
import { FriendGameInfo, FriendId, FriendIdentity, FriendPresence } from "./FriendComponents";
import { LoadingSkeleton } from "./LoadingSkeleton";
import { Icon } from "./Icon";
import { canJoinFriend } from "../lib/friends";
import { useWorkspaceActive } from "../hooks/useWorkspaceActive";

export function FriendDetailsPanel({viewerId,friend,busy,onCopy,onJoin,onManage,onProfile,onOpenGroup}: {
  viewerId:number; friend:Friend | null; busy:boolean; onCopy:(value:string,label:string)=>void; onJoin:(friend:Friend)=>void; onManage:(friend:Friend,action:"remove"|"block")=>void; onProfile:(id:number)=>void; onOpenGroup?:(group:{id:number;name:string})=>void;
}) {
  const [tab,setTab]=useState<FriendDetailTab>("details");
  const [loaded,setLoaded]=useState<{key:string;value:FriendDetails}|null>(null);
  const [loading,setLoading]=useState(false);
  const [error,setError]=useState<string|null>(null);
  const [retry,setRetry]=useState(0);
  const active=useWorkspaceActive();
  const target=friend?.userId;
  const key=`${viewerId}:${target}:${tab}`;
  useEffect(()=>{
    if (!target || !active) return;
    let current=true;setLoading(true);setError(null);
    void getFriendDetails(viewerId,target,tab).then(value=>{if(current)setLoaded({key:`${viewerId}:${target}:${tab}`,value});}).catch(reason=>{if(current)setError(operationError(reason,"Profile details are unavailable. Retry later."));}).finally(()=>{if(current)setLoading(false);});
    return()=>{current=false;};
  },[viewerId,target,tab,retry,active]);
  const details=loaded?.key===key?loaded.value:null;
  return <aside className="friends-details" aria-label="Friend details">
    {!friend?<div className="friends-empty"><strong>Select a friend</strong><span>Choose a row to see their profile and server details.</span></div>:<>
      <FriendIdentity user={friend}/><FriendPresence friend={friend}/>
      <div className="friends-detail-tabs" role="tablist" aria-label="Friend detail sections" onKeyDown={event=>{const values:FriendDetailTab[]=["details","mutual","groups"];const index=values.indexOf(tab);const next=event.key==="ArrowRight"?(index+1)%3:event.key==="ArrowLeft"?(index+2)%3:event.key==="Home"?0:event.key==="End"?2:null;if(next===null)return;event.preventDefault();setTab(values[next]);event.currentTarget.querySelectorAll<HTMLButtonElement>("button")[next]?.focus();}}>{([['details','Details'],['mutual','Mutual friends'],['groups','Groups']] as const).map(([value,label])=><button key={value} type="button" role="tab" tabIndex={tab===value?0:-1} aria-selected={tab===value} onClick={()=>setTab(value)}>{label}</button>)}</div>
      <div role="tabpanel" aria-label={tab==='details'?'Details':tab==='mutual'?'Mutual friends':'Groups'}>
        {loading&&!details&&<LoadingSkeleton layout="status" label="Loading friend details"/>}
        {error&&<div className="friends-error" role="alert">{error}{tab==='mutual'&&<><p>The connection list may be private or require verification. Open the signed-in profile to check access, then retry.</p><button type="button" className="account-button" onClick={()=>onProfile(friend.userId)}>Open profile to check access</button></>}<button type="button" className="account-button" onClick={()=>setRetry(value=>value+1)}>Retry</button></div>}
        {tab==='details'&&<><div className="friends-game-card"><FriendGameInfo friend={friend}/>{canJoinFriend(friend)&&<button className="account-button primary" type="button" disabled={busy} onClick={()=>onJoin(friend)}>Join game</button>}</div>
          <dl className="friends-facts"><dt>User ID</dt><dd><FriendId value={friend.userId} label="user ID" onCopy={onCopy}/></dd>{friend.placeId&&<><dt>Place ID</dt><dd><FriendId value={friend.placeId} label="Place ID" onCopy={onCopy}/></dd></>}<dt>Job ID</dt><dd><FriendId value={friend.jobId} label="Job ID" onCopy={onCopy}/></dd></dl>
          {details?.profile&&<section className="friends-profile-description"><h3>About</h3><p>{details.profile.description||"No profile description."}</p></section>}
          <p className="friends-muted">Friend-since dates, last-online times and server region/player counts are not provided by this integration.</p>
        </>}
        {tab==='mutual'&&details&&<><p className="friends-muted">{details.mutual.length} mutual friends visible to this account</p>{details.mutual.length?<ul className="friends-related">{details.mutual.map(user=><li key={user.userId}><FriendIdentity user={user}/></li>)}</ul>:<p>No visible mutual friends.</p>}</>}
        {tab==='groups'&&details&&<><p className="friends-muted">{details.groups.length} visible groups</p>{details.groups.length?<ul className="friends-related friends-group-list">{details.groups.map(group=><li key={group.id}><span className="friend-group-icon"><Icon name="groups"/>{group.iconUrl&&<img key={group.iconUrl} src={group.iconUrl} alt="" loading="lazy" decoding="async" onError={event=>{event.currentTarget.hidden=true;}}/>}</span><span className="friend-group-copy"><strong>{group.name}</strong><span className="friends-muted">{group.role}</span></span><button type="button" className="icon-button" aria-label={`Open ${group.name} in Groups`} data-tip={onOpenGroup?"Open in Groups":"Enable the Groups page in Settings to open this group"} disabled={busy||!onOpenGroup} onClick={()=>onOpenGroup?.(group)}><Icon name="link"/></button></li>)}</ul>:<p>No visible groups.</p>}</>}
      </div>
      <div className="friends-details-actions"><button type="button" className="account-button" onClick={()=>onProfile(friend.userId)}><Icon name="browser"/>Open profile in browser</button><button type="button" className="account-button danger" disabled={busy} onClick={()=>onManage(friend,'remove')}><Icon name="remove-group" tone="current-color"/>Remove friend</button><button type="button" className="account-button danger" disabled={busy} onClick={()=>onManage(friend,'block')}><Icon name="lock" tone="current-color"/>Block user</button></div>
    </>}
  </aside>;
}
