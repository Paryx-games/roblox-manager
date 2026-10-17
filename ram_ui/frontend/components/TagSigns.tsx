import type { CSSProperties } from "react";
import type { AccountTag } from "../lib/ipc";

export function TagSigns({ tags }: { tags: AccountTag[] }) {
  return (
    <span
      className={`tag-signs ${tags.length ? "" : "is-empty"}`}
      aria-hidden="true"
    >
      {tags.length ? (
        tags.slice(0, 3).map((tag) => (
          <span
            key={tag.id}
            className="tag-sign"
            style={
              {
                "--tag-color": `rgb(${(tag.color ?? [107, 112, 128]).join(", ")})`,
              } as CSSProperties
            }
          />
        ))
      ) : (
        <span className="tag-sign" />
      )}
      {tags.length > 3 && <span className="tag-overflow">+{tags.length - 3}</span>}
    </span>
  );
}
