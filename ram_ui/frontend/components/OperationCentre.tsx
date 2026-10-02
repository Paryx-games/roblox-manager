import { useState, useSyncExternalStore } from "react";
import { Popup } from "./Popup";
import { Icon } from "./Icon";
import Select from "./Select";
import { clearCompletedOperations, operationSnapshot, subscribeOperations, type OperationPage } from "../lib/operations";

const statusLabels = { pending: "In progress", success: "Succeeded", requested: "Requested", failed: "Failed", partial: "Needs attention" };

export function OperationCentre({ onClose, onNavigate, developerOptions }: {
  onClose: () => void;
  onNavigate: (page: OperationPage) => void;
  developerOptions: boolean;
}) {
  const entries = useSyncExternalStore(subscribeOperations, operationSnapshot);
  const [filter, setFilter] = useState("all");
  const visible = entries.filter((entry) => filter === "all" || filter === "pending" && entry.status === "pending" || filter === "attention" && (entry.status === "failed" || entry.status === "partial"));
  return <Popup className="confirm-modal operation-centre" backdropClassName="confirm-modal-backdrop" onClose={onClose} closeOnBackdrop labelledBy="operation-centre-title" describedBy="operation-centre-description">
    <div className="confirm-modal-header"><h2 id="operation-centre-title">Operation centre</h2><button className="icon-button" type="button" aria-label="Close operation centre" onClick={onClose}><Icon name="close" /></button></div>
    <p id="operation-centre-description" className="settings-muted">The latest 100 operation summaries from this session. History clears when RM exits. Requested means accepted, not necessarily running.</p>
    <div className="operation-centre-toolbar">
      <Select ariaLabel="Filter operations" value={filter} options={[{ value: "all", label: "All operations" }, { value: "pending", label: "In progress" }, { value: "attention", label: "Needs attention" }]} onChange={setFilter} />
      <button className="account-button" type="button" disabled={!entries.some((entry) => entry.status !== "pending")} onClick={clearCompletedOperations}>Clear completed</button>
    </div>
    <div className="operation-centre-results">
      {!visible.length ? <p role="status" className="settings-muted">{entries.length ? "No operations match this filter." : "No operations yet. Launches, imports, uploads, and settings changes will appear here."}</p> : <table className="assets-table"><caption className="sr-only">Recent operations</caption><thead><tr><th scope="col">Updated</th><th scope="col">Operation</th><th scope="col">Status</th><th scope="col">Workspace</th></tr></thead><tbody>{visible.map((entry) => <tr key={entry.id}>
        <td><time dateTime={new Date(entry.updatedAt).toISOString()} data-tip={new Date(entry.updatedAt).toLocaleString()}>{new Date(entry.updatedAt).toLocaleTimeString()}</time></td>
        <td><strong>{entry.title}</strong><small>{entry.detail}</small></td><td>{statusLabels[entry.status]}</td>
        <td><button className="account-button" type="button" disabled={entry.page === "Asset Manager" && !developerOptions} data-tip={entry.page === "Asset Manager" && !developerOptions ? "Enable developer options in Settings to open Asset Manager" : `Open ${entry.page}`} onClick={() => { onClose(); onNavigate(entry.page); }}>{entry.page}</button></td>
      </tr>)}</tbody></table>}
    </div>
  </Popup>;
}
