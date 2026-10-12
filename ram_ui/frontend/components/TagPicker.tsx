import Select from "./Select";
import type { AccountTag } from "../lib/ipc";

export function TagPicker({
  tags,
  selected,
  onChange,
  disabled = false,
  label,
}: {
  tags: AccountTag[];
  selected: string[];
  onChange: (ids: string[]) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <div className="account-tag-picker" aria-label={label} role="group">
      <Select
        value=""
        ariaLabel={`Add tag to ${label}`}
        disabled={disabled || tags.every((tag) => selected.includes(tag.id))}
        options={[
          {
            value: "",
            label: tags.length
              ? "Add tag..."
              : "Create a tag in Accounts first",
          },
          ...tags
            .filter((tag) => !selected.includes(tag.id))
            .map((tag) => ({ value: tag.id, label: tag.name })),
        ]}
        onChange={(id) => {
          if (id) onChange([...selected, id]);
        }}
      />
      <div className="account-tag-list">
        {selected.map((id) => (
          <button
            className="account-button"
            type="button"
            key={id}
            disabled={disabled}
            aria-label={`Remove ${tags.find((tag) => tag.id === id)?.name ?? "missing tag"} from ${label}`}
            onClick={() =>
              onChange(selected.filter((current) => current !== id))
            }
          >
            {tags.find((tag) => tag.id === id)?.name ?? "Missing tag"} - Remove
          </button>
        ))}
        {!selected.length && (
          <span className="text-muted">No tags selected</span>
        )}
      </div>
    </div>
  );
}
