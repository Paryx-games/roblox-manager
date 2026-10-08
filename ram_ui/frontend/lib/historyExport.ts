import type { HistoryEvent, HistoryKind } from "./ipc";
export const historyLabels: Record<HistoryKind, string> = {
  observed: "First observed", joined: "Joined game", left: "Left game", changedGame: "Changed game",
  changedServer: "Changed server", online: "Went online", offline: "Went offline", studio: "Entered Studio",
  leftStudio: "Left Studio", unknown: "Unknown presence", moderated: "Moderation detected",
};
export function historyCsv(events: HistoryEvent[]): string {
  const cell = (value: string | number | null) => {
    const text = String(value ?? "");
    const safe = /^[\s]*[=+@-]/.test(text) ? `'${text}` : text;
    return `"${safe.replace(/"/g, '""')}"`;
  };
  return ["observed_at,user_id,event,location,place_id,job_id", ...events.map(event =>
    [event.observedAt, event.userId, event.kind, event.location, event.placeId, event.jobId].map(cell).join(","),
  )].join("\r\n");
}
