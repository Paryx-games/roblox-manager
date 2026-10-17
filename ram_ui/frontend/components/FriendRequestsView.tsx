import { useWorkspaceActive } from "../hooks/useWorkspaceActive";
import { useEffect, useRef, useState } from "react";
import { getFriendRequests, operationError, type FriendUser, type FriendRequestsPage } from "../lib/ipc";
import { FriendIdentity } from "./FriendComponents";
import { LoadingSkeleton } from "./LoadingSkeleton";
import { friendLabel } from "../lib/friends";

export function FriendRequestsView({viewerId,search,revision,busy,onAction,onProfile,onCount}: {
  viewerId:number;search:string;revision:number;busy:boolean;onAction:(user:FriendUser,action:"accept"|"decline")=>void;onProfile:(id:number)=>void;onCount:(count:number,more:boolean)=>void;
}) {
  const active=useWorkspaceActive();
  const [state,setState]=useState<{viewer:number;page:FriendRequestsPage}|null>(null);
  const [loading,setLoading]=useState(false);const [error,setError]=useState<string|null>(null);
  const sequence=useRef(0);const seen=useRef(new Set<string>());
  const cached=useRef<{viewer:number;revision:number;loaded:number}|null>(null);
  const countCallback=useRef(onCount);countCallback.current=onCount;
  useEffect(()=>{
    if(!active){setLoading(false);return;}
    if(cached.current?.viewer===viewerId&&cached.current.revision===revision&&Date.now()-cached.current.loaded<30000){setLoading(false);return;}
    const version=++sequence.current;setLoading(true);setError(null);seen.current.clear();
    void getFriendRequests(viewerId,null,revision>0).then(page=>{if(sequence.current===version){cached.current={viewer:viewerId,revision,loaded:Date.now()};setState({viewer:viewerId,page});countCallback.current(page.users.length,!!page.nextCursor);}}).catch(reason=>{if(sequence.current===version)setError(operationError(reason,"Friend requests could not be loaded. Refresh to retry."));}).finally(()=>{if(sequence.current===version)setLoading(false);});
    return()=>{sequence.current+=1;};
  },[viewerId,revision,active]);
  const page=state?.viewer===viewerId?state.page:null;
  async function more(){
    if(!page?.nextCursor||loading)return;
    const version=sequence.current;const cursor=page.nextCursor;
    if(seen.current.has(cursor)){setError("Roblox repeated a request cursor. Refresh the request list to retry.");return;}
    setLoading(true);setError(null);
    try{const next=await getFriendRequests(viewerId,cursor);if(version!==sequence.current)return;seen.current.add(cursor);const users=[...new Map([...page.users,...next.users].map(user=>[user.userId,user])).values()];const merged={...next,users};cached.current={viewer:viewerId,revision,loaded:Date.now()};setState({viewer:viewerId,page:merged});countCallback.current(users.length,!!next.nextCursor);}
    catch(reason){if(version===sequence.current)setError(operationError(reason,"More requests could not be loaded. Try again."));}
    finally{if(version===sequence.current)setLoading(false);}
  }
  const query=search.trim().toLocaleLowerCase();
  const visible=(page?.users??[]).filter(user=>`${user.displayName} ${user.username}`.toLocaleLowerCase().includes(query));
  return <div className="friends-requests"><section aria-busy={loading}><h2>Incoming requests{page?` (${page.users.length}${page.nextCursor?"+":""})`:""}</h2>
    {loading&&page&&<p className="friends-muted" role="status">Refreshing requests...</p>}
    {error&&<p className="friends-error" role="alert">{error}</p>}
    {loading&&!page&&<LoadingSkeleton layout="results" label="Loading friend requests"/>}
    {page?.warnings.map(message=><p key={message} className="friends-muted" role="status">{message}</p>)}
    {page&&!visible.length&&!loading&&<p className="friends-muted">{query?"No loaded requests match this search.":"No incoming requests."}</p>}
    <ul className="friends-request-list">{visible.map(user=><li key={user.userId}><button className="friend-name-button" type="button" aria-label={`Open ${friendLabel(user)}'s profile in browser`} onClick={()=>onProfile(user.userId)}><FriendIdentity user={user}/></button><div><button className="account-button primary" type="button" disabled={busy||loading} onClick={()=>onAction(user,'accept')}>Accept</button><button className="account-button" type="button" disabled={busy||loading} onClick={()=>onAction(user,'decline')}>Decline</button></div></li>)}</ul>
    {page?.nextCursor&&<><p className="friends-muted">Search includes loaded requests. Load more to search the remaining requests.</p><button type="button" className="account-button" disabled={loading||busy} onClick={()=>void more()}>{loading?"Loading...":"Load more requests"}</button></>}
  </section><section className="friends-outgoing"><h2>Outgoing requests</h2><p className="friends-muted">Outgoing requests are not available here. Manage these on Roblox.</p><button className="account-button" type="button" onClick={()=>onProfile(viewerId)}>Open Roblox profile</button></section></div>;
}
