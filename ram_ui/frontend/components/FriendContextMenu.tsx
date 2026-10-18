import { useWorkspaceActive } from "../hooks/useWorkspaceActive";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { AccountAvatar } from "./AccountAvatar";
import { PopupMenu } from "./PopupMenu";
import { Icon } from "./Icon";
import { canJoinFriend, friendLabel } from "../lib/friends";
import type { Friend } from "../lib/ipc";

export function FriendContextMenu({friend, busy, multiInstance, onCopy, onJoin, onManage, onView, onProfile}: {
  friend:Friend; busy:boolean; multiInstance:boolean; onCopy:(value:string,label:string)=>void;
  onJoin:(friend:Friend, expectedJobId?:string,requireNewInstance?:boolean)=>void; onManage:(friend:Friend,action:"remove"|"block")=>void; onView:()=>void; onProfile:(id:number)=>void;
}) {
  const [open,setOpen] = useState(false);
  const active=useWorkspaceActive();
  useEffect(()=>{if(!active)setOpen(false);},[active]);
  const [position,setPosition] = useState({top:0,left:0});
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (!open) return;
    const outside = (event:PointerEvent) => { if (event.target instanceof Node && !trigger.current?.contains(event.target) && !menu.current?.contains(event.target)) setOpen(false); };
    document.addEventListener("pointerdown",outside,true);
    return () => document.removeEventListener("pointerdown",outside,true);
  },[open]);
  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const rect=trigger.current?.getBoundingClientRect(); const bounds=menu.current?.getBoundingClientRect();
      if (!rect || !bounds) return;
      setPosition({left:Math.max(8,Math.min(rect.right-bounds.width,window.innerWidth-bounds.width-8)),top:Math.max(8,rect.bottom+bounds.height+4>window.innerHeight ? rect.top-bounds.height-4 : rect.bottom+4)});
    };
    place(); window.addEventListener("resize",place); window.addEventListener("scroll",place,true);
    return () => {window.removeEventListener("resize",place); window.removeEventListener("scroll",place,true);};
  },[open]);
  function act(action:()=>void) {setOpen(false); trigger.current?.focus(); action();}
  return <><button ref={trigger} type="button" className="icon-button" aria-label={`Options for ${friendLabel(friend)}`} aria-haspopup="menu" aria-expanded={open} disabled={busy} data-tip="Friend options" onClick={()=>setOpen(value=>!value)}><Icon name="more" /></button>
    {open && active && createPortal(<PopupMenu ariaLabel={`Friend options for ${friendLabel(friend)}`} className="friends-context-menu" style={{position:"fixed",...position}} menuRef={element=>{menu.current=element;}} onClose={()=>setOpen(false)}>
      <div className="friends-context-identity"><AccountAvatar account={{...friend,label:friendLabel(friend)}} className="account-picker-avatar"/><span><strong>{friendLabel(friend)}</strong><small>{friend.username?`@${friend.username}`:`User ${friend.userId}`}</small></span></div><hr/>
      {canJoinFriend(friend) && <><button type="button" role="menuitem" onClick={()=>act(()=>onJoin(friend))}><Icon name="launch"/>Join game</button>{multiInstance && <button type="button" role="menuitem" onClick={()=>act(()=>onJoin(friend,undefined,true))}><Icon name="windows"/>Join in new instance</button>}{friend.jobId && <button type="button" role="menuitem" onClick={()=>act(()=>onJoin(friend,friend.jobId!))}><Icon name="link"/>Join same server</button>}<hr /></>}
      {friend.username&&<button type="button" role="menuitem" onClick={()=>act(()=>onCopy(friend.username,"username"))}><Icon name="copy"/>Copy username</button>}
      <button type="button" role="menuitem" onClick={()=>act(()=>onCopy(String(friend.userId),"user ID"))}><Icon name="copy"/>Copy user ID</button>
      {friend.jobId && <button type="button" role="menuitem" onClick={()=>act(()=>onCopy(friend.jobId!,"Job ID"))}><Icon name="copy"/>Copy Job ID</button>}
      <hr /><button type="button" role="menuitem" onClick={()=>act(onView)}><Icon name="accounts"/>View profile</button><button type="button" role="menuitem" onClick={()=>act(()=>onProfile(friend.userId))}><Icon name="browser"/>Open profile in browser</button>
      <hr /><button type="button" role="menuitem" className="friends-danger" onClick={()=>act(()=>onManage(friend,"remove"))}><Icon name="remove-group" tone="current-color"/>Remove friend</button><button type="button" role="menuitem" className="friends-danger" onClick={()=>act(()=>onManage(friend,"block"))}><Icon name="lock" tone="current-color"/>Block user</button>
    </PopupMenu>,document.body)}
  </>;
}
