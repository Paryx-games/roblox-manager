import { useEffect, useState, type RefObject } from "react";
import { Icon } from "./Icon";
import { useWorkspaceState } from "../hooks/useWorkspaceState";

const keywords: Record<string, string> = {
  "privacy-cleanup": "cookies local storage full profile exit clipboard",
  "window-layout": "monitor display grid columns rows padding arrange rename",
  "app-startup": "startup windows revalidate auto launch account",
  "launch-safeguards": "multi instance background tray kill confirmation",
  "network-identity": "mac adapter rotation oui",
  "displayed-identity": "anonymize names avatars",
  "discord-notifications": "webhook notifications discord test",
  "development": "developer inventories asset manager cache",
  "launch-pacing": "delay launch pacing",
};

type SearchResult = { id: string; label: string; text: string };

export function SettingsSearch({ contentRef, onNavigate }: {
  contentRef: RefObject<HTMLElement>;
  onNavigate: (id: string) => void;
}) {
  const [query, setQuery] = useWorkspaceState("settings.search", "");
  const [index, setIndex] = useState<SearchResult[]>([]);
  useEffect(() => {
    const content = contentRef.current;
    if (!content) return;
    const sections = Array.from(content.querySelectorAll<HTMLElement>(".settings-section"));
    setIndex(sections.flatMap((section) => {
      const title = section.querySelector("h2")?.textContent ?? "Settings";
      const headings = Array.from(section.querySelectorAll<HTMLElement>("h3[data-settings-anchor]"));
      if (!headings.length) return [{ id: section.dataset.settingsAnchor!, label: title, text: section.textContent ?? "" }];
      return headings.map((heading, i) => {
        let text = `${title} ${keywords[heading.dataset.settingsAnchor!] ?? ""}`;
        for (let node: Element | null = heading; node && node !== headings[i + 1]; node = node.nextElementSibling) text += ` ${node.textContent ?? ""}`;
        return { id: heading.dataset.settingsAnchor!, label: `${title} / ${heading.textContent}`, text };
      });
    }));
  }, [contentRef]);
  const terms = query.trim().toLocaleLowerCase().split(/\s+/);
  const results = index.filter((item) => terms.every((term) => item.text.toLocaleLowerCase().includes(term)));
  return <div className="settings-search">
    <label className="settings-field-row"><Icon name="search" /><input aria-label="Search settings" placeholder="Search settings…" value={query} onChange={(event) => setQuery(event.target.value)} /></label>
    {query.trim() && <div className="settings-search-results" aria-label="Settings search results">
      {!results.length && <p role="status">No matching settings. Try another word.</p>}
      {results.map((item) => <button key={item.id} className="account-button" type="button" onClick={() => onNavigate(item.id)}>{item.label}</button>)}
    </div>}
  </div>;
}
