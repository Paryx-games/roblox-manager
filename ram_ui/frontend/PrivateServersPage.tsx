import { useEffect, useMemo, useState } from "react";
import {
  addPrivateServer,
  launchPrivateServer,
  listPrivateServers,
  removePrivateServer,
  type PrivateServerSummary,
} from "./lib/ipc";

function Icon({ name }: { name: string }) {
  return <img className="account-icon" src={`/icons/${name}.svg`} alt="" aria-hidden="true" />;
}

export function PrivateServersPage({ selectedIds }: { selectedIds: Set<number> }) {
  const [servers, setServers] = useState<PrivateServerSummary[]>([]);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  async function reload() {
    setLoading(true);
    try { setServers(await listPrivateServers()); setError(null); }
    catch { setError("Private servers could not be loaded."); }
    finally { setLoading(false); }
  }
  useEffect(() => { void reload(); }, []);
  const groups = useMemo(() => {
    const map = new Map<number, PrivateServerSummary[]>();
    servers.forEach((server) => map.set(server.placeId, [...(map.get(server.placeId) ?? []), server]));
    return [...map.entries()];
  }, [servers]);
  async function addServer() {
    setSaving(true); setError(null);
    try { const server = await addPrivateServer(name, url); setServers((current) => [...current, server]); setName(""); setUrl(""); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "The private server could not be added."); }
    finally { setSaving(false); }
  }
  async function launch(index: number) {
    try { await launchPrivateServer(index, [...selectedIds]); }
    catch (reason) { setError(reason instanceof Error ? reason.message : "The private server could not be launched."); }
  }
  async function remove(index: number) {
    if (!window.confirm("Remove this private server?")) return;
    try { await removePrivateServer(index); await reload(); }
    catch { setError("The private server could not be removed."); }
  }
  return <>
    <div className="header-row"><h1 className="header-title">Private Servers</h1></div>
    <main className="private-servers-page">
      <section className="private-server-card" aria-labelledby="add-private-server"><h2 id="add-private-server">Add Private Server</h2>
        <div className="private-server-fields"><label>Name<input value={name} onChange={(event) => setName(event.target.value)} placeholder="e.g. Grinding Server" /></label><label>URL<input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="Paste private server link" /></label></div>
        <button className="account-button primary" type="button" disabled={saving || !name.trim() || !url.trim()} onClick={() => void addServer()}><Icon name="add" />{saving ? "Adding..." : "Add Server"}</button>
      </section>
      <section className="private-server-card" aria-labelledby="saved-private-servers"><h2 id="saved-private-servers">Saved Private Servers</h2>
        {error && <p className="private-server-error" role="alert">{error}</p>}
        {loading ? <p className="common-inventory-summary">Loading private servers...</p> : groups.length === 0 ? <div className="account-empty-inline"><Icon name="game" /><strong>No private servers saved yet</strong><span>Add one above to start launching straight into it.</span></div> : groups.map(([placeId, group]) => <div className="private-server-group" key={placeId}><div className="private-server-group-head"><Icon name="game" /><div><strong>{group[0].placeName || `Place ${placeId}`}</strong><span>{group.length} server{group.length === 1 ? "" : "s"}</span></div></div>{group.map((server) => <div className="private-server-row" key={server.index}><strong>{server.name}</strong><div><button className="account-button primary" type="button" disabled={!selectedIds.size} onClick={() => void launch(server.index)}><Icon name="launch" />Launch</button><button className="icon-button" type="button" aria-label={`Remove ${server.name}`} data-tip="Delete" onClick={() => void remove(server.index)}><Icon name="delete" /></button></div></div>)}</div>)}
      </section>
    </main>
  </>;
}
