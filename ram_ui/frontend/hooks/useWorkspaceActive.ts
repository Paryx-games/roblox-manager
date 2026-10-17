import { createContext, useContext } from "react";

export const WorkspaceActiveContext = createContext(true);
export function useWorkspaceActive() { return useContext(WorkspaceActiveContext); }
