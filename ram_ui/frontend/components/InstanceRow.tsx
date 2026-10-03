import { useId, type ReactNode } from "react";
import type { AccountSummary, InstanceSummary } from "../lib/ipc";
import { AccountAvatar } from "./AccountAvatar";
import { Icon } from "./Icon";

export function instanceKey(instance: InstanceSummary) {
  return `${instance.pid}:${instance.startTime}`;
}

export function InstanceIdentity({ instance, account }: { instance: InstanceSummary; account?: AccountSummary }) {
  return <span className="instance-identity">
    {account ? <AccountAvatar account={account} className="instance-avatar" /> : <span className="instance-avatar"><Icon name="app-window" /></span>}
    <span className="instance-identity-copy"><strong>{instance.label}</strong><span className="selectable-text">PID {instance.pid}{instance.userId !== null && <> · User {instance.userId}</>}</span></span>
  </span>;
}

export function InstanceRow({ instance, account, selected, onSelect, actions }: {
  instance: InstanceSummary;
  account?: AccountSummary;
  selected: boolean;
  onSelect: () => void;
  actions: ReactNode;
}) {
  const detailsId = useId();
  const matchDescription = instance.attribution === "exact" ? "Verified from this client's launch token. Individual close is available." : instance.attribution === "inferred" ? "Estimated from launch order. Individual close is disabled." : "RM could not identify this client's account. Individual close and server join are disabled.";
  return <li className="instance-row" data-selected={selected}>
    <button type="button" className="instance-row-select" aria-pressed={selected} aria-label={`Select ${instance.label}, PID ${instance.pid}`} aria-describedby={detailsId} aria-description={matchDescription} onClick={onSelect}>
      <InstanceIdentity instance={instance} account={account} />
      <span className="instance-details" id={detailsId}>
        <span><span>Place ID</span><span className="selectable-text">{instance.placeId ?? "Unknown"}</span></span>
        <span><span>Match</span><span className="instances-attribution" data-attribution={instance.attribution} data-tip={matchDescription}>{instance.attribution === "exact" ? "Exact" : instance.attribution === "inferred" ? "Inferred" : "Unmatched"}</span></span>
        <span><span>Launched</span><span>{instance.launchedAt ? new Date(instance.launchedAt).toLocaleString() : "Unknown"}</span></span>
      </span>
    </button>
    <div className="instance-row-actions">{actions}</div>
  </li>;
}
