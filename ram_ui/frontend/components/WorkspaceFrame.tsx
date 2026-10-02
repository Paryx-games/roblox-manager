import { useLayoutEffect, useRef, type ReactNode } from "react";

const scrollPositions = new Map<string, Map<string, [number, number]>>();

export function WorkspaceFrame({ page, children }: { page: string; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const root = ref.current;
    if (!root) return;
    const saved = scrollPositions.get(page) ?? new Map<string, [number, number]>();
    scrollPositions.set(page, saved);
    const pending = new Map(saved);
    const paneKey = (element: HTMLElement) => element.classList[0] ?? element.tagName;
    let frame = 0;
    function restore() {
      const loading = root!.querySelector('[aria-busy="true"], .loading-skeleton');
      for (const element of root!.querySelectorAll<HTMLElement>("[class]")) {
        const key = paneKey(element);
        const position = pending.get(key);
        if (!position || element.closest("[data-rm-overlay]")) continue;
        if (loading && element.scrollHeight - element.clientHeight < position[0]) continue;
        element.scrollTop = position[0];
        element.scrollLeft = position[1];
        pending.delete(key);
      }
      if (!pending.size) observer.disconnect();
    }
    function scheduleRestore() {
      if (!pending.size) return;
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(restore);
    }
    function remember(event: Event) {
      const element = event.target;
      if (!(element instanceof HTMLElement) || element.closest("[data-rm-overlay]")) return;
      const key = paneKey(element);
      if (!pending.has(key)) saved.set(key, [element.scrollTop, element.scrollLeft]);
    }
    function takeControl() { pending.clear(); observer.disconnect(); }
    const observer = new MutationObserver(scheduleRestore);
    if (pending.size) observer.observe(root, { childList: true, subtree: true, attributes: true, attributeFilter: ["aria-busy"] });
    root.addEventListener("scroll", remember, true);
    root.addEventListener("wheel", takeControl, { passive: true });
    root.addEventListener("pointerdown", takeControl);
    root.addEventListener("keydown", takeControl);
    root.addEventListener("focusin", takeControl);
    scheduleRestore();
    return () => {
      cancelAnimationFrame(frame);
      observer.disconnect();
      root.removeEventListener("scroll", remember, true);
      root.removeEventListener("wheel", takeControl);
      root.removeEventListener("pointerdown", takeControl);
      root.removeEventListener("keydown", takeControl);
      root.removeEventListener("focusin", takeControl);
    };
  }, [page]);

  return <div ref={ref} className="page-transition-layer">{children}</div>;
}
