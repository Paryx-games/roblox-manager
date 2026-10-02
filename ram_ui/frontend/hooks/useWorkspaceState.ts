import { useEffect, useState, type Dispatch, type SetStateAction } from "react";

// explicit call sites retain only view preferences, never credential fields
const workspaceValues = new Map<string, unknown>();

export function useWorkspaceState<T>(key: string, initial: T | (() => T)): [T, Dispatch<SetStateAction<T>>] {
  const [value, setValue] = useState<T>(() => workspaceValues.has(key)
    ? workspaceValues.get(key) as T
    : typeof initial === "function" ? (initial as () => T)() : initial);
  useEffect(() => { workspaceValues.set(key, value); }, [key, value]);
  return [value, setValue];
}
