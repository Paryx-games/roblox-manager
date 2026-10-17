import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { AccountPicker } from "./components/AccountPicker";
import { FriendRow } from "./components/FriendComponents";
import { FriendDetailsPanel } from "./components/FriendDetailsPanel";
import { FriendRequestsView } from "./components/FriendRequestsView";
import { JoinFriendDialog } from "./components/JoinFriendDialog";
import { ConfirmModal } from "./ConfirmModal";
import { LoadingSkeleton } from "./components/LoadingSkeleton";
import { Icon } from "./components/Icon";
import Select from "./components/Select";
import { Toast, type ToastItem } from "./Toast";
import { useWorkspaceActive } from "./hooks/useWorkspaceActive";
import { useFriendsData } from "./hooks/useFriendsData";
import { useSelectAllShortcut } from "./hooks/useSelectAllShortcut";
import { clampFriendsSidebar, canJoinFriend, friendCounts, friendLabel, reconcileFriendsSelection, visibleFriends, type FriendFilter, type FriendSort } from "./lib/friends";
import { friendAction, browseAsAccount, operationError, type AccountSummary, type Friend, type FriendAction, type FriendUser } from "./lib/ipc";

const accountKey="rm.friends.account";
const filters: {key:FriendFilter;label:string}[]=[{key:'all',label:'All'},{key:'playing',label:'Playing'},{key:'studio',label:'In Studio'},{key:'online',label:'Online'},{key:'offline',label:'Offline'},{key:'requests',label:'Requests'}];
const sorts=[{value:'status',label:'Online status first'},{value:'nameAsc',label:'Name A-Z'},{value:'nameDesc',label:'Name Z-A'},{value:'userAsc',label:'Username A-Z'},{value:'userDesc',label:'Username Z-A'}];

export function FriendsPage({accounts,accountsReady,accountsError,onRetryAccounts,multiInstanceEnabled}: {accounts:AccountSummary[];accountsReady:boolean;accountsError:string|null;onRetryAccounts:()=>Promise<void>;multiInstanceEnabled:boolean}) {
  const [viewer,setViewer]=useState<number|null>(()=>{try{const id=Number(localStorage.getItem(accountKey));return Number.isSafeInteger(id)&&id>0?id:null;}catch{return null;}});
  const [retryingAccounts,setRetryingAccounts]=useState(false);
  const [picker,setPicker]=useState(false);const [search,setSearch]=useState('');const [filter,setFilter]=useState<FriendFilter>('all');const [sort,setSort]=useState<FriendSort>('status');
  const [selected,setSelected]=useState(new Set<number>());const [focused,setFocused]=useState<number|null>(null);
  const [error,setError]=useState<string|null>(null);const [toast,setToast]=useState<ToastItem|null>(null);
  const [busy,setBusy]=useState(false);const pending=useRef(false);const mounted=useRef(true);
  const [confirm,setConfirm]=useState<{user:FriendUser;action:FriendAction}|null>(null);
  const [join,setJoin]=useState<{friend:Friend;expectedJobId:string|null;requireNewInstance:boolean}|null>(null);
  const [requestsRevision,setRequestsRevision]=useState(0);const [requestCount,setRequestCount]=useState<{viewer:number;count:number;more:boolean}|null>(null);
  const active=useWorkspaceActive();const selectAll=useRef<HTMLInputElement>(null);const initialized=useRef(false);
  const account=accounts.find(account=>account.userId===viewer);
  const allowed=!!account&&!account.cookieExpired;
  const [sidebarWidth,setSidebarWidth]=useState(320);
  const [workspaceWidth,setWorkspaceWidth]=useState(1000);
  const workspace=useRef<HTMLDivElement>(null);
  const resize=useRef<{x:number;width:number}|null>(null);
  const effectiveSidebarWidth=clampFriendsSidebar(sidebarWidth,workspaceWidth);
  function resizeSidebar(width:number){setSidebarWidth(clampFriendsSidebar(width,workspaceWidth));}
  useEffect(()=>{
    const element=workspace.current;
    if(!element || !active)return;
    const observer=new ResizeObserver(entries=>{setWorkspaceWidth(entries[0]?.contentRect.width??element.clientWidth);});
    observer.observe(element);
    return()=>{observer.disconnect();resize.current=null;};
  },[active,viewer,filter,accountsReady,allowed]);
  const {data,loading,error:loadError,refresh}=useFriendsData(viewer,active,allowed);
  const friends=data?.friends;
  const rows=useMemo(()=>visibleFriends(friends??[],search,filter,sort),[friends,search,filter,sort]);
  const counts=useMemo(()=>friendCounts(friends??[]),[friends]);
  const currentFriend=friends?.find(friend=>friend.userId===focused)??null;
  const checkedFriends=(friends??[]).filter(friend=>selected.has(friend.userId));
  const allChecked=rows.length>0&&rows.every(friend=>selected.has(friend.userId));
  const someChecked=rows.some(friend=>selected.has(friend.userId));
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
  useEffect(()=>{if(!accountsReady||accountsError)return;const initial=!initialized.current;initialized.current=true;setViewer(current=>accounts.some(account=>account.userId===current)?current:initial||current!==null?accounts[0]?.userId??null:null);},[accounts,accountsReady,accountsError]);
  useEffect(()=>{
    if(!accountsReady||accountsError)return;
    try{if(viewer)localStorage.setItem(accountKey,String(viewer));else localStorage.removeItem(accountKey);}catch{setToast({id:Date.now(),kind:"warning",title:"Account selection not saved",message:"The selected account could not be remembered. You can still use Friends for this session.",duration:"standard"});}
  },[viewer,accountsReady,accountsError]);
  useEffect(()=>{setSelected(new Set());setFocused(null);setJoin(null);setConfirm(null);setError(null);},[viewer]);
  useEffect(()=>{if(!friends)return;setSelected(current=>reconcileFriendsSelection(current,friends));setFocused(current=>friends.some(friend=>friend.userId===current)?current:null);},[friends]);
  useEffect(()=>{if(selectAll.current)selectAll.current.indeterminate=someChecked&&!allChecked;},[someChecked,allChecked]);
  function toggleVisible(){setSelected(current=>{const next=new Set(current);for(const row of rows){if(allChecked)next.delete(row.userId);else next.add(row.userId);}return next;});}
  useSelectAllShortcut(()=>setSelected(current=>new Set([...current,...rows.map(friend=>friend.userId)])),{isEnabled:active&&filter!=='requests'&&rows.length>0});
  function notify(title:string,message:string){setToast({id:Date.now(),title,message,kind:'success',duration:'standard'});}
  async function copy(value:string,label:string){try{await navigator.clipboard.writeText(value);if(mounted.current){setError(null);notify('Copied',`${label} copied to the clipboard.`);}}catch{if(mounted.current)setError(`Could not copy ${label}. Check clipboard access and try again.`);}}
  const onCopy=(value:string,label:string)=>{void copy(value,label);};
  async function openProfile(id:number){if(!viewer)return;try{await browseAsAccount(viewer,false,id);}catch(reason){if(mounted.current)setError(operationError(reason,'The Roblox profile popup could not be opened.'));}}
  function startJoin(friend:Friend,expectedJobId?:string,requireNewInstance=false){if(canJoinFriend(friend)){setError(null);setJoin({friend,expectedJobId:expectedJobId??null,requireNewInstance});}}
  async function mutate(user:FriendUser,action:FriendAction){
    if(!viewer||pending.current)return;
    const owner=viewer;pending.current=true;setBusy(true);setError(null);
    try{await friendAction(owner,user.userId,action);if(!mounted.current)return;setConfirm(null);refresh();setRequestsRevision(value=>value+1);notify(action==='accept'?'Friend request accepted':action==='decline'?'Friend request declined':action==='block'?'User blocked':'Friend removed','Refreshing the account to show current relationships.');}
    catch(reason){if(mounted.current)setError(operationError(reason,'The friend action failed. Refresh before trying again.'));}
    finally{pending.current=false;if(mounted.current)setBusy(false);}
  }
  function manage(user:FriendUser,action:FriendAction){setError(null);if(action==='accept')void mutate(user,action);else setConfirm({user,action});}
  function reload(){refresh();setRequestsRevision(value=>value+1);setError(null);}
  const requestBadge=requestCount?.viewer===viewer?`${requestCount.count}${requestCount.more?'+':''}`:null;
  return <><main className="friends-page" data-walkthrough="friends">
    <header className="friends-header"><div><h1 className="header-title">Friends</h1><p>View and manage your Roblox friends for the selected account.</p></div><button type="button" className="account-button" disabled={!allowed||loading||busy} onClick={reload}><Icon name="refresh"/>{loading?'Refreshing...':'Refresh'}</button></header>
    <div className="friends-account-bar"><div><span className="friends-account-label">Account</span><AccountPicker accounts={accounts} mode="single" selectedId={viewer??undefined} onSelectedIdChange={id=>setViewer(id??null)} open={picker} onOpenChange={setPicker} disabled={busy||!accountsReady} isLoading={!accountsReady} ariaLabel="Friends account"/></div><div className="friends-summary" aria-live="polite">{data?<><strong>{counts.all} friends</strong>{counts.unknown>0&&<span>{counts.unknown} unavailable</span>}<small>{data.refreshedAt?`Updated ${new Date(data.refreshedAt).toLocaleTimeString([], {hour:"2-digit",minute:"2-digit"})}`:'Presence refresh incomplete'}{loading?' - refreshing cached data':''}</small></>:<span>{loading?'Loading friends...':viewer?'No data loaded':'Choose an account'}</span>}</div></div>
    <div className="friends-toolbar"><label className="friends-search"><Icon name="search"/><input type="search" aria-label="Search friends" placeholder="Search friends..." value={search} onChange={event=>setSearch(event.target.value)}/></label><div className="friends-filters" role="group" aria-label="Friend status filters">{filters.map(item=><button key={item.key} type="button" aria-pressed={filter===item.key} onClick={()=>setFilter(item.key)}>{item.label}{item.key==='requests'?(requestBadge!==null?` (${requestBadge})`:''):data?` (${counts[item.key as keyof typeof counts]})`:''}</button>)}</div><Select ariaLabel="Sort friends" value={sort} options={sorts} onChange={value=>setSort(value as FriendSort)} disabled={filter==='requests'}/></div>
    {accountsError&&<div className="friends-page-error" role="alert">{accountsError}<button type="button" className="account-button" disabled={retryingAccounts} onClick={()=>{setRetryingAccounts(true);void onRetryAccounts().finally(()=>{if(mounted.current)setRetryingAccounts(false);});}}>{retryingAccounts?"Retrying...":"Retry accounts"}</button></div>}
    {(error||loadError)&&<div className="friends-page-error" role="alert">{error||loadError}{data?' Showing previously loaded data.':''}<button type="button" className="account-button" disabled={loading||busy||!allowed} onClick={reload}>Retry</button></div>}
    {data?.warnings.length? <div className="friends-warnings" role="status">{[...new Set(data.warnings)].join(' ')}</div>:null}
    {!accountsReady?<LoadingSkeleton layout="accounts" label="Loading managed accounts"/>:accountsError&&!account?<div className="friends-empty"><strong>Accounts unavailable</strong><span>Retry the account list above.</span></div>:!viewer?<div className="friends-empty"><strong>No account selected</strong><span>Add or unlock an account in Accounts to browse friends.</span></div>:!allowed?<div className="friends-empty" role="alert"><strong>Account login unavailable</strong><span>Refresh this account or log in again from Accounts.</span></div>:filter==='requests'?<FriendRequestsView viewerId={viewer} search={search} revision={requestsRevision} busy={busy||loading} onAction={manage} onProfile={id=>void openProfile(id)} onCount={(count,more)=>setRequestCount({viewer,count,more})}/>:<>
      {checkedFriends.length>0&&<div className="friends-selection" aria-label="Selected friend actions"><strong>{checkedFriends.length} selected</strong><button type="button" className="account-button" disabled={checkedFriends.some(friend=>!friend.username)} onClick={()=>void copy(checkedFriends.filter(friend=>friend.username).map(friend=>friend.username).join('\n'),'usernames')}>Copy usernames</button>{checkedFriends.some(friend=>!friend.username)&&<span className="friends-muted">Some usernames are unavailable; copy user IDs instead.</span>}<button type="button" className="account-button" onClick={()=>void copy(checkedFriends.map(friend=>friend.userId).join('\n'),'user IDs')}>Copy user IDs</button><button type="button" className="account-button" onClick={()=>setSelected(new Set())}>Clear selection</button></div>}
      <div className="friends-workspace" ref={workspace} style={{ "--friends-sidebar-width": `${effectiveSidebarWidth}px` } as CSSProperties}><section className="friends-list-panel" aria-label="Friends list" aria-busy={loading}>
        {loading&&!data?<LoadingSkeleton layout="results" label="Loading friends and presence"/>:!data?<div className="friends-empty"><span>Friends have not loaded. Use Refresh to try again.</span></div>:!rows.length?<div className="friends-empty"><strong>{!friends?.length?'No friends yet':search?'No matching friends':'No friends in this category'}</strong><span>{search?'Try another display name or username.':'Choose another filter or refresh the selected account.'}</span></div>:<div className="friends-list-scroll"><table className="friends-table"><caption className="sr-only">Friends, presence and available server details</caption><thead><tr><th><input ref={selectAll} type="checkbox" checked={allChecked} aria-label="Select all visible friends" onChange={toggleVisible}/></th><th scope="col">Friend</th><th scope="col">Status</th><th scope="col">Game / Place</th><th scope="col">Job ID</th><th scope="col">Actions</th></tr></thead><tbody>{rows.map(friend=><FriendRow key={friend.userId} friend={friend} checked={selected.has(friend.userId)} selected={focused===friend.userId} busy={busy||loading} multiInstance={multiInstanceEnabled} onCheck={()=>{setFocused(friend.userId);setSelected(current=>{const next=new Set(current);if(next.has(friend.userId))next.delete(friend.userId);else next.add(friend.userId);return next;});}} onSelect={()=>setFocused(friend.userId)} onCopy={onCopy} onJoin={startJoin} onManage={manage} onProfile={id=>void openProfile(id)}/>)}</tbody></table></div>}
      </section><div className="friends-resizer" role="separator" aria-label="Resize friend details" aria-orientation="vertical" aria-valuemin={240} aria-valuemax={clampFriendsSidebar(520,workspaceWidth)} aria-valuenow={effectiveSidebarWidth} tabIndex={0}
        onPointerDown={event=>{if(event.button!==0)return;resize.current={x:event.clientX,width:effectiveSidebarWidth};event.currentTarget.setPointerCapture(event.pointerId);}}
        onPointerMove={event=>{if(resize.current)resizeSidebar(resize.current.width+resize.current.x-event.clientX);}}
        onPointerUp={()=>{resize.current=null;}} onPointerCancel={()=>{resize.current=null;}} onLostPointerCapture={()=>{resize.current=null;}}
        onKeyDown={event=>{if(['ArrowLeft','ArrowRight','Home','End'].includes(event.key)){event.preventDefault();resizeSidebar(event.key==='Home'?240:event.key==='End'?520:effectiveSidebarWidth+(event.key==='ArrowLeft'?16:-16));}}}/><FriendDetailsPanel viewerId={viewer} friend={currentFriend} busy={busy||loading} onCopy={onCopy} onJoin={startJoin} onManage={manage} onProfile={id=>void openProfile(id)}/></div>
    </>}
  </main>{confirm&&<ConfirmModal title={confirm.action==='block'?'Block this user?':confirm.action==='decline'?'Decline this friend request?':'Remove this friend?'} message={<><p>{`${confirm.action==='block'?'Block':confirm.action==='decline'?'Decline the request from':'Remove'} ${friendLabel(confirm.user)} (${confirm.user.username?`@${confirm.user.username}`:`User ${confirm.user.userId}`}) for ${account?.username??'this account'}? This changes your relationship on Roblox.`}</p>{error&&<p className="friends-error" role="alert">{error}</p>}</>} confirmLabel={busy?'Working...':confirm.action==='block'?'Block user':confirm.action==='decline'?'Decline request':'Remove friend'} confirmDisabled={busy} busy={busy} onCancel={()=>{if(!busy)setConfirm(null);}} onConfirm={()=>void mutate(confirm.user,confirm.action)}/>}
  {join&&viewer&&<JoinFriendDialog friend={join.friend} expectedJobId={join.expectedJobId} requireNewInstance={join.requireNewInstance} viewerId={viewer} accounts={accounts} externalError={error} onCopy={onCopy} onClose={()=>setJoin(null)} onRequested={()=>{setJoin(null);notify('Launch requested','Roblox is still connecting. Check Instances to confirm the account entered the game.');}}/>}
  {toast&&<Toast item={toast} onDismiss={()=>setToast(null)}/>}</>;
}
