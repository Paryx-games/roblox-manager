import { useEffect, useRef, useState } from "react";
import { getFriends, operationError, type FriendsSnapshot } from "../lib/ipc";

export function useFriendsData(userId: number | null, active: boolean, allowed: boolean) {
  const cache = useRef(new Map<number, {data: FriendsSnapshot; loaded: number}>());
  const [loaded, setLoaded] = useState<{userId: number; data: FriendsSnapshot} | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const force = useRef<number | null>(null);
  useEffect(() => {
    let current = true;
    setError(null);
    if (!userId || !allowed) { setLoaded(null); setLoading(false); if (!userId) cache.current.clear(); return; }
    const cached = cache.current.get(userId);
    setLoaded(cached ? {userId, data: cached.data} : null);
    if (!active) { setLoading(false); return; }
    const refresh = force.current === userId;
    force.current = null;
    if (!refresh && cached && Date.now() - cached.loaded < 30000) { setLoading(false); return; }
    setLoading(true);
    void getFriends(userId, refresh).then(data => {
      if (!current) return;
      if (cache.current.size >= 32 && !cache.current.has(userId)) cache.current.delete(cache.current.keys().next().value!);
      const observed = data.refreshedAt ? Date.parse(data.refreshedAt) : Date.now();
      cache.current.set(userId, {data, loaded: Number.isFinite(observed) ? Math.min(observed, Date.now()) : Date.now()});
      setLoaded({userId, data});
    }).catch(reason => { if (current) setError(operationError(reason, "Friends could not be loaded. Refresh to retry.")); }).finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [userId, active, allowed, revision]);
  function refresh() { force.current = userId; setRevision(value => value + 1); }
  return {data: loaded?.userId === userId && allowed ? loaded.data : null, loading, error, refresh};
}
