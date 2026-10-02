export function parseLaunchDestination(input: string): number | null {
  const text = input.trim();
  let id = text;
  if (!/^\d+$/.test(text)) {
    try {
      const url = new URL(text);
      if (url.protocol !== "https:" || !["roblox.com", "www.roblox.com"].includes(url.hostname)
        || url.username || url.password || url.port || url.search || url.hash) return null;
      const match = /^\/(?:[a-z]{2}(?:-[a-z]{2})?\/)?games\/(\d+)(?:\/[^/]*)?\/?$/i.exec(url.pathname);
      if (!match) return null;
      id = match[1];
    } catch { return null; }
  }
  const value = Number(id);
  return Number.isSafeInteger(value) && value > 0 ? value : null;
}
