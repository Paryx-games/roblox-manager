import { useEffect, useRef, useState } from "react";
import { Popup } from "./Popup";
import { AccountPicker } from "./AccountPicker";
import { Icon } from "./Icon";
import Select from "./Select";
import { exportValidation } from "../lib/accountExport";
import { useWorkspaceActive } from "../hooks/useWorkspaceActive";
import { exportAccounts, operationError, type AccountSummary, type AccountExportOptions, type AccountExportField, type AccountExportFormat, type AccountExportEncryption } from "../lib/ipc";

const fieldOptions: {key: AccountExportField; label: string}[] = [
  {key:"username",label:"Usernames"}, {key:"accountName",label:"Account names (alias or username)"},
  {key:"displayName",label:"Display names"}, {key:"userId",label:"User IDs"}, {key:"alias",label:"Aliases"},
  {key:"group",label:"Groups"}, {key:"createdAt",label:"Creation dates"}, {key:"accountAge",label:"Account age (hours)"}, {key:"lastUsed",label:"Last used dates"},
];

export function AccountExportDialog({accounts, initialIds, onClose, onExported}: {
  accounts: AccountSummary[]; initialIds: ReadonlySet<number>; onClose:()=>void; onExported:(count:number, encrypted:boolean)=>void;
}) {
  const [selected,setSelected]=useState(()=>new Set(accounts.filter(account=>!initialIds.size || initialIds.has(account.userId)).map(account=>account.userId)));
  const [picker,setPicker]=useState(false);
  const [format,setFormat]=useState<AccountExportFormat>("json");
  const [encryption,setEncryption]=useState<AccountExportEncryption>("none");
  const [fields,setFields]=useState<AccountExportField[]>(["username","accountName","displayName","userId","alias","group","createdAt","lastUsed"]);
  const [password,setPassword]=useState(""); const [confirm,setConfirm]=useState(""); const [visible,setVisible]=useState(false);
  const [busy,setBusy]=useState(false); const pending=useRef(false); const [error,setError]=useState<string|null>(null);
  const active=useWorkspaceActive();
  useEffect(()=>{if(!active){setPassword("");setConfirm("");onClose();}},[active,onClose]);
  const options: AccountExportOptions = {accountIds:accounts.filter(account=>selected.has(account.userId)).map(account=>account.userId), fields, format, encryption, password};
  const validation=exportValidation(options,confirm);
  const cookies=fields.includes("cookie");
  function toggleField(field:AccountExportField, checked:boolean) {
    setFields(current=>checked?[...current,field]:current.filter(item=>item!==field));
    if(field==="cookie" && checked && encryption==="none")setEncryption("aes256Gcm");
    setError(null);
  }
  function chooseEncryption(method:AccountExportEncryption) {
    if(cookies && method==="none")return;
    setEncryption(method);setError(null);
    if(method==="none"){setPassword("");setConfirm("");setVisible(false);}
  }
  async function save() {
    if(pending.current || validation)return;
    pending.current=true;setBusy(true);setPicker(false);setError(null);
    try {
      const result=await exportAccounts(options);
      if(result){setPassword("");setConfirm("");onExported(result.count,result.encrypted);onClose();}
    } catch(reason){setError(operationError(reason,"The export could not be saved. No credentials were returned to this window."));}
    finally{pending.current=false;setBusy(false);}
  }
  if(!active)return null;
  return <Popup className="confirm-modal account-export-dialog" backdropClassName="confirm-modal-backdrop" labelledBy="account-export-title" busy={busy} onClose={onClose}>
    <div className="account-export-heading"><h2 id="account-export-title">Export to</h2><button className="icon-button" type="button" aria-label="Close export" disabled={busy} onClick={onClose}><Icon name="close"/></button></div>
    <label className="account-export-field">File format<Select ariaLabel="Export file format" value={format} options={[{value:"json",label:"JSON"},{value:"csv",label:"CSV"},{value:"txt",label:"TXT"}]} onChange={value=>setFormat(value as AccountExportFormat)} disabled={busy}/></label>
    <div className="account-export-field"><span>Accounts ({options.accountIds.length} selected)</span><AccountPicker accounts={accounts} mode="multiple" showAllAccounts open={picker} onOpenChange={setPicker} selectedIds={selected} onSelectedIdsChange={setSelected} disabled={busy} ariaLabel="Accounts to export" unselectedLabel="Select accounts to export"/></div>
    <fieldset className="account-export-fields" disabled={busy}><legend>Include fields</legend>{fieldOptions.map(field=><label key={field.key}><input type="checkbox" checked={fields.includes(field.key)} onChange={event=>toggleField(field.key,event.target.checked)}/>{field.label}</label>)}<label className="account-export-cookie"><input type="checkbox" checked={cookies} onChange={event=>toggleField("cookie",event.target.checked)}/>Roblox cookies (.ROBLOSECURITY)</label></fieldset>
    <p className="settings-muted">Exports original saved values, including when account names are anonymized in RM. Cookies can only be written to an encrypted export.</p>
    <fieldset className="account-export-encryption" disabled={busy}><legend>Encryption (optional)</legend><span className="settings-muted">Recommended - authenticated encryption with Argon2id</span>
      {([{value:"aes256Gcm",label:"AES-256-GCM"},{value:"aes128Gcm",label:"AES-128-GCM"}] as const).map(method=><label className="friends-radio-account" key={method.value}><input type="radio" name="export-encryption" value={method.value} checked={encryption===method.value} onChange={()=>chooseEncryption(method.value)}/><span>{method.label}</span><small>Recommended</small></label>)}
      <div className="account-export-unprotected"><label className="friends-radio-account"><input type="radio" name="export-encryption" value="none" checked={encryption==="none"} disabled={cookies} onChange={()=>chooseEncryption("none")}/>No encryption - unprotected</label><small>{cookies?"Unavailable when exporting cookies.":"Anyone with this file can read the selected account data."}</small></div>
    </fieldset>
    <label className="account-export-field" htmlFor="account-export-password">Export password</label>
    <div className="account-export-password"><input id="account-export-password" type={visible?"text":"password"} autoComplete="off" spellCheck={false} value={password} disabled={busy||encryption==="none"} onChange={event=>setPassword(event.target.value)} placeholder="At least 12 characters"/><Icon name="lock"/><button className="icon-button" type="button" disabled={busy||encryption==="none"} aria-label={visible?"Hide encryption password":"Show encryption password"} aria-pressed={visible} onClick={()=>setVisible(current=>!current)}><Icon name="eye"/></button></div>
    {encryption!=="none"&&<><label className="account-export-field">Confirm export password<input type={visible?"text":"password"} autoComplete="off" spellCheck={false} value={confirm} disabled={busy} onChange={event=>setConfirm(event.target.value)}/></label><p className="settings-muted">Keep this password. It is separate from your RM password and cannot be recovered. Encrypted exports use RM's .rmexport format and contain your selected JSON, CSV or TXT data.</p></>}
    {validation&&<p className="settings-muted" role="status">{validation}</p>}{error&&<p className="friends-error" role="alert">{error}</p>}
    <div className="confirm-modal-actions"><button className="account-button" type="button" disabled={busy} onClick={onClose}>Cancel</button><button className="account-button primary" type="button" disabled={busy||!!validation} onClick={()=>void save()}>{busy?"Exporting...":`Export ${options.accountIds.length} account${options.accountIds.length===1?"":"s"}`}</button></div>
  </Popup>;
}
