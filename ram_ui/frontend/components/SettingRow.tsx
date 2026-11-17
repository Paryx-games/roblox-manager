import { useEffect, useId, useState, type ReactNode } from "react";

export function SettingRow({ label, description, children }: {
  label: string;
  description: string;
  children: (labelId: string, descriptionId: string) => ReactNode;
}) {
  const id = useId();
  return <div className="control-setting-row">
    <div className="control-setting-copy">
      <strong id={`${id}-label`}>{label}</strong>
      <p id={`${id}-description`}>{description}</p>
    </div>
    <div className="control-setting-input">{children(`${id}-label`, `${id}-description`)}</div>
  </div>;
}

export function RangeField({ value, min, max, onChange, labelId, descriptionId, disabled = false }: {
  value: number;
  min: number;
  max: number;
  onChange: (value: number) => void;
  labelId: string;
  descriptionId: string;
  disabled?: boolean;
}) {
  return <div className="control-range-field">
    <input type="range" min={min} max={max} step={1} value={value} onChange={(event) => onChange(Number(event.target.value))} aria-labelledby={labelId} aria-describedby={descriptionId} disabled={disabled} />
    <output aria-labelledby={labelId}>{value}</output>
  </div>;
}

export function RangeNumberField({ value, min, max, unit, onChange, labelId, descriptionId, disabled = false }: {
  value: number;
  min: number;
  max: number;
  unit: string;
  onChange: (value: number) => void;
  labelId: string;
  descriptionId: string;
  disabled?: boolean;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);

  function commit() {
    const parsed = Number(text);
    const next = Number.isFinite(parsed) ? Math.min(max, Math.max(min, Math.round(parsed))) : value;
    setText(String(next));
    onChange(next);
  }

  return <div className="control-range-number-field">
    <input type="range" min={min} max={max} step={1} value={value} onChange={(event) => onChange(Number(event.target.value))} aria-labelledby={labelId} aria-describedby={descriptionId} disabled={disabled} />
    <input type="number" min={min} max={max} step={1} value={text} onChange={(event) => {
      const next = event.currentTarget.value;
      setText(next);
      const parsed = Number(next);
      if (next !== "" && Number.isInteger(parsed) && parsed >= min && parsed <= max) onChange(parsed);
    }} onBlur={commit} onKeyDown={(event) => { if (event.key === "Enter") commit(); }} aria-labelledby={labelId} aria-describedby={descriptionId} disabled={disabled} />
    <span>{unit}</span>
  </div>;
}

export function ToggleField({ checked, onChange, labelId, descriptionId, disabled = false }: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  labelId: string;
  descriptionId: string;
  disabled?: boolean;
}) {
  return <div className={`settings-toggle ${disabled ? "is-disabled" : ""}`}>
    <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} aria-labelledby={labelId} aria-describedby={descriptionId} disabled={disabled} />
  </div>;
}
