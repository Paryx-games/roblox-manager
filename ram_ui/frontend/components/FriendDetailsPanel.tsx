import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getFriendDetails, operationError, type Friend, type FriendDetails, type FriendDetailTab, type LaunchProgress } from "../lib/ipc";
import { FriendGameInfo, FriendId, FriendIdentity, FriendPresence, JoinUnavailable } from "./FriendComponents";
import { LoadingSkeleton } from "./LoadingSkeleton";
import { Icon } from "./Icon";
import { canJoinFriend } from "../lib/friends";
import { useWorkspaceActive } from "../hooks/useWorkspaceActive";

const launchRecoveryHints: Record<string, string> = {
  invalid_account_or_target: "Choose an account and a valid Place ID.",
  invalid_launch_input: "Review the Place ID, Job ID, and launch data.",
  invalid_batch_size: "Select between 1 and 500 accounts.",
  account_missing: "Refresh Accounts and select an available account.",
  account_restricted_or_expired: "Refresh or re-add this account in Accounts.",
  account_credential_unavailable: "Unlock the account store or re-add this account.",
  multi_instance_disabled: "Enable multi-instance in Settings or retry without a new instance.",
  startup_settings_changed: "Review the startup launch settings and try again.",
  player_executable_missing: "Check the Roblox installation path in Settings.",
  custom_arguments_invalid: "Review the custom Roblox arguments in Settings.",
  fast_flags_failed: "Check the Roblox installation and FastFlag settings.",
  privacy_cleanup_failed: "Review the privacy cleanup settings, then retry.",
  mac_rotation_blocked: "Close Roblox clients before rotating the network adapter address.",
  mac_rotation_failed: "Check adapter support and administrator permission, or disable MAC rotation.",
  launch_cancelled: "The account or app state changed while starting. Retry the launch.",
  private_server_access_failed: "Confirm this account has access to the private server.",
  roblox_auth_rejected: "Refresh the account and review any Roblox login requirements.",
  process_launch_failed: "Check the Roblox installation and try launching again.",
  launch_queue_failed: "Wait for the current launch to finish, then retry.",
  roblox_request_failed: "Check the account and connection, then retry.",
  launch_preflight_failed: "Review the account and launch settings, then retry.",
  launch_preparation_failed: "Review the launch settings, then retry.",
};

export function FriendDetailsPanel({viewerId,friend,busy,onCopy,onJoin,onManage,onProfile,onOpenGroup}: {
  viewerId:number; friend:Friend | null; busy:boolean; onCopy:(value:string,label:string)=>void; onJoin:(friend:Friend)=>void; onManage:(friend:Friend,action:"remove"|"block")=>void; onProfile:(id:number)=>void; onOpenGroup?:(group:{id:number;name:string})=>void;
}) {
  const [tab,setTab]=useState<FriendDetailTab>("details");
  const [loaded,setLoaded]=useState<{key:string;value:FriendDetails}|null>(null);
  const [loading,setLoading]=useState(false);
  const [error,setError]=useState<string|null>(null);
  const [retry,setRetry]=useState(0);
  const [launchStatus,setLaunchStatus]=useState<LaunchProgress|null>(null);
  const active=useWorkspaceActive();
  const target=friend?.userId;
  const key=`${viewerId}:${target}:${tab}`;
  useEffect(()=>{
    if (!target || !active) return;
    let current=true;setLoading(true);setError(null);
    void getFriendDetails(viewerId,target,tab).then(value=>{if(current)setLoaded({key:`${viewerId}:${target}:${tab}`,value});}).catch(reason=>{if(current)setError(operationError(reason,"Profile details are unavailable. Retry later."));}).finally(()=>{if(current)setLoading(false);});
    return()=>{current=false;};
  },[viewerId,target,tab,retry,active]);
  useEffect(()=>{
    let current=true;
    let clearTimer=0;
    let stop:(()=>void)|null=null;
    void listen<LaunchProgress>("launch-progress",event=>{
      if(!current)return;
      window.clearTimeout(clearTimer);
      setLaunchStatus(event.payload);
      if(["started","failed","blocked"].includes(event.payload.phase)){
        const duration = event.payload.phase === "started" ? 5000 : 8000;
        clearTimer=window.setTimeout(()=>{if(current)setLaunchStatus(null);},duration);
      }
    }).then(unlisten=>{if(current)stop=unlisten;else unlisten();});
    return()=>{current=false;window.clearTimeout(clearTimer);stop?.();};
  },[]);
  const details=loaded?.key===key?loaded.value:null;
  const launchLabel=launchStatus?{
    queued:"Launch queued",
    starting:"Starting Roblox",
    retrying:"Retrying Roblox request",
    started:"Roblox started",
    failed:"Launch failed",
    blocked:"Launch blocked",
  }[launchStatus.phase]:null;
  const launchHint=launchStatus?.reasonCode
    ? launchRecoveryHints[launchStatus.reasonCode] ?? `Check the RM log for reason code ${launchStatus.reasonCode}.`
    : null;
  return <aside className="friends-details" aria-label="Friend details">
    {!friend?<div className="friends-empty"><strong>Select a friend</strong><span>Choose a row to see their profile and server details.</span></div>:<>
      <FriendIdentity user={friend}/><FriendPresence friend={friend}/>
      {launchStatus&&launchLabel&&<p className={`friends-launch-status friends-launch-status-${launchStatus.phase}`} role="status" data-tip={launchStatus.reasonCode ? `Reason code: ${launchStatus.reasonCode}` : launchLabel}><span className="friends-launch-dot" aria-hidden="true"/><span>{launchLabel}</span>{launchHint&&<small>{launchHint}</small>}</p>}
      <div className="friends-detail-tabs" role="tablist" aria-label="Friend detail sections" onKeyDown={event=>{const values:FriendDetailTab[]=["details","mutual","groups"];const index=values.indexOf(tab);const next=event.key==="ArrowRight"?(index+1)%3:event.key==="ArrowLeft"?(index+2)%3:event.key==="Home"?0:event.key==="End"?2:null;if(next===null)return;event.preventDefault();setTab(values[next]);event.currentTarget.querySelectorAll<HTMLButtonElement>("button")[next]?.focus();}}>{([['details','Details'],['mutual',`Mutual${details&&tab==='mutual'?` (${details.mutual.length})`:''}`],['groups',`Groups${details&&tab==='groups'?` (${details.groups.length})`:''}`]] as const).map(([value,label])=><button key={value} type="button" role="tab" tabIndex={tab===value?0:-1} aria-selected={tab===value} onClick={()=>setTab(value)}>{label}</button>)}</div>
      <div role="tabpanel" aria-label={tab==='details'?'Details':tab==='mutual'?'Mutual friends':'Groups'}>
        {loading&&!details&&<LoadingSkeleton layout="status" label="Loading friend details"/>}
        {error&&<div className="friends-error" role="alert">{error}{tab==='mutual'&&<><p>The connection list may be private or require verification. Open the signed-in profile to check access, then retry.</p><button type="button" className="account-button" onClick={()=>onProfile(friend.userId)}>Open profile to check access</button></>}<button type="button" className="account-button" onClick={()=>setRetry(value=>value+1)}>Retry</button></div>}
        {tab==='details'&&<><div className="friends-game-card"><FriendGameInfo friend={friend}/>{canJoinFriend(friend)?<button className="account-button primary" type="button" disabled={busy} onClick={()=>onJoin(friend)}>Join game</button>:friend.status==='playing'?<JoinUnavailable/>:null}</div>
          <dl className="friends-facts"><dt>User ID</dt><dd><FriendId value={friend.userId} label="user ID" onCopy={onCopy}/></dd>{friend.placeId&&<><dt>Place ID</dt><dd><FriendId value={friend.placeId} label="Place ID" onCopy={onCopy}/></dd></>}<dt>Job ID</dt><dd><FriendId value={friend.jobId} label="Job ID" onCopy={onCopy}/></dd></dl>
          {details?.profile&&<section className="friends-profile-description"><h3>About</h3><p>{details.profile.description||"No profile description."}</p></section>}
          <p className="friends-muted">Friend-since dates, last-online times and server region/player counts are not provided by this integration.</p>
        </>}
        {tab==='mutual'&&details&&<><p className="friends-muted">{details.mutual.length} mutual friends visible to this account</p>{details.mutual.length?<ul className="friends-related">{details.mutual.map(user=><li key={user.userId}><FriendIdentity user={user}/></li>)}</ul>:<p>No visible mutual friends.</p>}</>}
        {tab==='groups'&&details&&<><p className="friends-muted">{details.groups.length} groups visible to this friend. Roles describe their membership, not the selected account’s access.</p>{details.groups.length?<ul className="friends-related friends-group-list">{details.groups.map(group=><li key={group.id}><span className="friend-group-icon"><Icon name="groups"/>{group.iconUrl&&<img key={group.iconUrl} src={group.iconUrl} alt="" loading="lazy" decoding="async" onError={event=>{event.currentTarget.hidden=true;}}/>}</span><span className="friend-group-copy"><strong>{group.name}</strong><span className="friends-muted">{group.role}</span></span><button type="button" className="icon-button friends-group-goto" aria-label={`Go to ${group.name} in Groups`} data-tip={onOpenGroup?"Go to Groups":"Enable the Groups page in Settings to open this group"} disabled={busy||!onOpenGroup} onClick={()=>onOpenGroup?.(group)}><Icon name="square-arrow-out-up-right"/><span>Go to</span></button></li>)}</ul>:<p>No visible groups.</p>}</>}
      </div>
      <div className="friends-details-actions"><button type="button" className="account-button" onClick={()=>onProfile(friend.userId)}><Icon name="browser"/>Open profile in browser</button><button type="button" className="account-button danger" disabled={busy} onClick={()=>onManage(friend,'remove')}><Icon name="remove-group" tone="current-color"/>Remove friend</button><button type="button" className="account-button danger" disabled={busy} onClick={()=>onManage(friend,'block')}><Icon name="lock" tone="current-color"/>Block user</button></div>
    </>}
  </aside>;
}
