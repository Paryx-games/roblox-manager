import { useEffect, useId, useRef, useState } from "react";
import { getSettings, joinFriendGame, operationError, type AccountSummary, type Friend } from "../lib/ipc";
import { Popup } from "./Popup";
import { ConfirmModal } from "../ConfirmModal";
import { AccountPicker } from "./AccountPicker";
import { FriendGameInfo, FriendId } from "./FriendComponents";
import { friendLabel } from "../lib/friends";
import { LoadingSkeleton } from "./LoadingSkeleton";

export function JoinFriendDialog({friend,expectedJobId,requireNewInstance,accounts,viewerId,externalError,onCopy,onClose,onRequested}: {
  friend:Friend; expectedJobId:string|null; requireNewInstance:boolean; accounts:AccountSummary[]; viewerId:number; externalError:string|null; onCopy:(value:string,label:string)=>void; onClose:()=>void; onRequested:(requested:number)=>void;
}) {
  const titleId=useId();
  const eligible=(account:AccountSummary)=>!account.cookieExpired&&account.canLaunch&&account.userId!==friend.userId;
  const eligibleAccounts=accounts.filter(eligible);
  const eligibleIds=eligibleAccounts.map(account=>account.userId);
  const disabledIds=new Set(accounts.filter(account=>!eligible(account)).map(account=>account.userId));
  const [selectedIds,setSelectedIds]=useState<Set<number>>(()=>new Set([accounts.find(account=>account.userId===viewerId&&eligible(account))?.userId??accounts.find(eligible)?.userId].filter((id):id is number=>id!==undefined)));
  const [pickerOpen,setPickerOpen]=useState(false);
  const [multi,setMulti]=useState<boolean|null>(null);
  const [sessionRisk,setSessionRisk]=useState(false);
  const [retryConfirmation,setRetryConfirmation]=useState(false);
  const [error,setError]=useState<string|null>(null);
  const [results,setResults]=useState<Array<{userId:number;error:string|null}>>([]);
  const [busy,setBusy]=useState(false);
  const [confirm,setConfirm]=useState(false);
  const [retry,setRetry]=useState(0);
  const pending=useRef(false); const mounted=useRef(true);
  useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;};},[]);
  useEffect(()=>{
    let current=true;setMulti(null);setError(null);
    void getSettings().then(snapshot=>{if(current){setMulti(snapshot.config.multiInstanceEnabled);setSessionRisk(snapshot.robloxRunning);}}).catch(reason=>{if(current)setError(operationError(reason,"Launch settings could not be loaded. Retry before joining."));});
    return()=>{current=false;};
  },[retry]);
  const chosen=eligibleAccounts.filter(account=>selectedIds.has(account.userId));
  async function launch(reuseConfirmed:boolean) {
    if(pending.current||!chosen.length||multi===null||(requireNewInstance&&!multi))return;
    pending.current=true;setBusy(true);setPickerOpen(false);setConfirm(false);setError(null);setResults([]);
    const failed:number[]=[];
    const nextResults=results.filter(result=>!selectedIds.has(result.userId));
    try {
      for(const account of chosen) {
        if(!mounted.current)return;
        try {
          await joinFriendGame(account.userId,friend.userId,expectedJobId,reuseConfirmed,requireNewInstance);
          nextResults.push({userId:account.userId,error:null});
        } catch(reason) {
          const message=operationError(reason,"The launch could not be requested. Refresh the friend's session and retry.");
          failed.push(account.userId);
          nextResults.push({userId:account.userId,error:message});
        }
        if(mounted.current)setResults([...nextResults]);
      }
      if(!mounted.current)return;
      if(failed.length) {
        setSelectedIds(new Set(failed));
        setRetryConfirmation(true);
        setError(`${chosen.length-failed.length} of ${chosen.length} launch requests succeeded. Only failed accounts remain selected.`);
        try {
          const snapshot=await getSettings();
          if(mounted.current){setMulti(snapshot.config.multiInstanceEnabled);setSessionRisk(snapshot.robloxRunning);}
        } catch {
          if(mounted.current)setMulti(null);
        }
      } else {
        onRequested(chosen.length);
      }
    }
    finally {pending.current=false;if(mounted.current)setBusy(false);}
  }
  return <><Popup className="confirm-modal friends-join-dialog" backdropClassName="confirm-modal-backdrop" labelledBy={titleId} busy={busy} onClose={onClose}>
    <div className="confirm-modal-header"><h2 id={titleId}>Join {friendLabel(friend)}'s game</h2></div>
    <FriendGameInfo friend={friend}/>
    <dl className="friends-facts"><dt>Place ID</dt><dd><FriendId value={friend.placeId} label="Place ID" onCopy={onCopy}/></dd><dt>Job ID</dt><dd><FriendId value={friend.jobId} label="Job ID" onCopy={onCopy}/></dd></dl>
    {expectedJobId&&<p className="friends-muted">Join this exact server. If your friend moved, refresh before trying again.</p>}
    <div className="friends-join-accounts"><span>Join with account</span><AccountPicker accounts={accounts} mode="multiple" showAllAccounts ariaLabel="Accounts to join" open={pickerOpen} onOpenChange={setPickerOpen} selectedIds={selectedIds} onSelectedIdsChange={setSelectedIds} disabledAccountIds={disabledIds} disabled={busy||!eligibleIds.length} unselectedLabel="Select accounts to join"/></div>
    {!!results.length&&<ul className="friends-join-results" aria-live="polite">{results.map(result=><li key={result.userId} data-failed={result.error!==null}><strong>{accounts.find(account=>account.userId===result.userId)?.label??`User ${result.userId}`}</strong><span>{result.error??"Launch requested."}</span></li>)}</ul>}
    {multi===null&&!error&&<LoadingSkeleton layout="status" label="Checking launch settings"/>}
    {multi!==null&&<><label className="friends-muted friends-instance-policy"><input type="checkbox" checked={multi} disabled/>Open in new instance (RM setting)</label><p className="friends-muted">{multi?"Multi-instance is enabled. RM will request a new Roblox client.":"Multi-instance is disabled. Roblox may reuse or replace an existing client. Confirmation is required. Change this policy in Settings."} Joining still depends on Roblox permissions.</p></>}
    {requireNewInstance&&multi===false&&<p className="friends-error" role="alert">Enable Multi-instance in Settings before requesting a new instance.</p>}
    {(sessionRisk||retryConfirmation)&&<p className="friends-muted">Roblox is running or a previous launch may still be starting. Another login to this account may disconnect its existing session.</p>}
    {externalError&&<p className="friends-error" role="alert">{externalError}</p>}
    {error&&<p className="friends-error" role="alert">{error}{multi===null&&<button type="button" className="account-button" onClick={()=>setRetry(value=>value+1)}>Retry settings</button>}</p>}
    <div className="confirm-modal-actions"><button type="button" className="account-button" disabled={busy} onClick={onClose}>Cancel</button><button type="button" className="account-button primary" disabled={busy||multi===null||!chosen.length||(requireNewInstance&&!multi)} onClick={()=>{if(multi&&!sessionRisk&&!retryConfirmation)void launch(false);else setConfirm(true);}}>{busy?"Requesting launches...":`Join ${chosen.length} account${chosen.length===1?"":"s"}`}</button></div>
  </Popup>{confirm&&<ConfirmModal title={multi?"Continue with another session possible?":"Continue with Multi-instance disabled?"} message={`Roblox may reuse or replace an existing client, or disconnect another session for each selected account. RM will not close clients itself. Continue with the saved launch policy for ${chosen.length} account${chosen.length===1?"":"s"}?`} confirmLabel={`Continue with ${chosen.length} account${chosen.length===1?"":"s"}`} confirmIcon="launch" onCancel={()=>setConfirm(false)} onConfirm={()=>void launch(true)}/>}</>;
}
