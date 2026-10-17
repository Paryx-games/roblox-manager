import { Icon } from "./Icon";

export function InfoTip({ text, kind = "info" }: { text: string; kind?: string }) {
  return <button className={`settings-info settings-info-${kind}`} type="button" aria-label={`${kind.charAt(0).toUpperCase() + kind.slice(1)}: ${text}`} data-tip={text}>
    <Icon name="shield-question-mark" tone="current-color" />
  </button>;
}
