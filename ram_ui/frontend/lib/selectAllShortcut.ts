type ShortcutKey = { key: string; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean; isComposing: boolean };

export function isSelectAllShortcut(event: ShortcutKey): boolean {
  return (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "a" && !event.altKey && !event.shiftKey && !event.isComposing;
}

export function isTextSelectionTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (target instanceof HTMLElement && target.isContentEditable) return true;
  if (target.closest('textarea, [role="textbox"]')) return true;
  if (!(target instanceof HTMLInputElement)) return false;
  return !["button", "checkbox", "radio", "submit", "reset", "range", "color", "file", "hidden", "image"].includes(target.type);
}
