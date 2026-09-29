import type { ReactNode } from "react";

function formatInline(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*|`[^`]+`|~~[^~]+~~)/g).map((part, index) => {
    if (part.startsWith("**")) return <strong key={index}>{part.slice(2, -2)}</strong>;
    if (part.startsWith("`")) return <code key={index}>{part.slice(1, -1)}</code>;
    if (part.startsWith("~~")) return <del key={index}>{part.slice(2, -2)}</del>;
    return part;
  });
}

export function ReleaseNotes({ markdown }: { markdown: string }) {
  const blocks = markdown.trim().split(/\r?\n\s*\r?\n/);
  return (
    <div className="startup-changelog selectable-text" tabIndex={0} aria-label="Release notes">
      {blocks.map((block, index) => {
        if (block.startsWith("## ")) return <h2 key={index}>{block.slice(3)}</h2>;
        if (block.startsWith("### ")) return <h3 key={index}>{block.slice(4)}</h3>;
        if (block.startsWith("- ")) {
          const items = block.split(/\r?\n(?=- )/);
          return <ul key={index}>{items.map((item, itemIndex) => <li key={itemIndex}>{formatInline(item.slice(2).replace(/\r?\n/g, " "))}</li>)}</ul>;
        }
        return <p key={index}>{formatInline(block.replace(/\r?\n/g, " "))}</p>;
      })}
    </div>
  );
}
