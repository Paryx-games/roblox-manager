import type { ResourceMonitorSettingsUpdate } from "./ipc";

export function hasResourceMonitorSettingsChanged(
  draft: ResourceMonitorSettingsUpdate,
  saved: ResourceMonitorSettingsUpdate,
): boolean {
  return draft.cpuLimitPercent !== saved.cpuLimitPercent
    || draft.memoryLimitMb !== saved.memoryLimitMb
    || draft.cpuAction !== saved.cpuAction
    || draft.memoryAction !== saved.memoryAction;
}
