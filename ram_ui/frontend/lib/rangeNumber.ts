export function committedRangeNumber(text: string, current: number, min: number, max: number): number {
  const parsed = Number(text);
  return Number.isFinite(parsed) ? Math.min(max, Math.max(min, Math.round(parsed))) : current;
}

export function validRangeNumberDraft(text: string, min: number, max: number): number | null {
  if (text === "") return null;
  const parsed = Number(text);
  return Number.isInteger(parsed) && parsed >= min && parsed <= max ? parsed : null;
}

export function commitRangeNumberOnEvent(
  eventType: "blur" | "keydown",
  key: string | undefined,
  commit: () => void,
): void {
  if (eventType === "blur" || (eventType === "keydown" && key === "Enter")) commit();
}
