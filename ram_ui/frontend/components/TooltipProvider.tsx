import { useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";

export function TooltipProvider() {
  const tooltipId = useId();
  const tooltipRef = useRef<HTMLDivElement>(null);
  const [target, setTarget] = useState<HTMLElement | null>(null);
  const [text, setText] = useState("");
  const [position, setPosition] = useState({ top: 0, left: 0 });

  useEffect(() => {
    let hoveredTarget: HTMLElement | null = null;
    let focusedTarget: HTMLElement | null = null;
    let pendingTarget: HTMLElement | null = null;
    let hoverTimer: number | undefined;
    function findTarget(node: EventTarget | null) {
      return node instanceof Element ? node.closest<HTMLElement>("[data-tip]") : null;
    }
    function updateTarget() {
      const nextTarget = hoveredTarget ?? focusedTarget;
      if (nextTarget === pendingTarget) return;
      window.clearTimeout(hoverTimer);
      pendingTarget = nextTarget;
      setTarget(null);
      if (!nextTarget?.isConnected) return;
      if (nextTarget === focusedTarget) {
        setTarget(nextTarget);
        return;
      }
      hoverTimer = window.setTimeout(() => {
        if (nextTarget.isConnected) setTarget(nextTarget);
      }, 500);
    }
    function onPointerOver(event: PointerEvent) {
      hoveredTarget = findTarget(event.target);
      updateTarget();
    }
    function onPointerOut(event: PointerEvent) {
      hoveredTarget = findTarget(event.relatedTarget);
      updateTarget();
    }
    function onFocusIn(event: FocusEvent) {
      focusedTarget = findTarget(event.target);
      updateTarget();
    }
    function onFocusOut(event: FocusEvent) {
      focusedTarget = findTarget(event.relatedTarget);
      updateTarget();
    }
    function dismissTooltip() {
      hoveredTarget = null;
      focusedTarget = null;
      pendingTarget = null;
      window.clearTimeout(hoverTimer);
      setTarget(null);
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") dismissTooltip();
    }
    document.addEventListener("pointerover", onPointerOver);
    document.addEventListener("pointerout", onPointerOut);
    document.addEventListener("focusin", onFocusIn);
    document.addEventListener("focusout", onFocusOut);
    document.addEventListener("pointerdown", dismissTooltip);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("blur", dismissTooltip);
    return () => {
      window.clearTimeout(hoverTimer);
      document.removeEventListener("pointerover", onPointerOver);
      document.removeEventListener("pointerout", onPointerOut);
      document.removeEventListener("focusin", onFocusIn);
      document.removeEventListener("focusout", onFocusOut);
      document.removeEventListener("pointerdown", dismissTooltip);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("blur", dismissTooltip);
    };
  }, []);

  useLayoutEffect(() => {
    if (!target) { setText(""); return; }
    function updateText() {
      if (!target?.isConnected) { setTarget(null); return; }
      setText(target.dataset.tip ?? "");
    }
    updateText();
    const observer = new MutationObserver(updateText);
    observer.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["data-tip"] });
    return () => observer.disconnect();
  }, [target]);

  useLayoutEffect(() => {
    if (!target || !text) return;
    const descriptions = (target.getAttribute("aria-describedby") ?? "").split(/\s+/).filter(Boolean);
    target.setAttribute("aria-describedby", [...descriptions, tooltipId].join(" "));
    function positionTooltip() {
      const tooltip = tooltipRef.current;
      if (!target || !tooltip) return;
      const inset = Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--space-2"));
      const bounds = target.getBoundingClientRect();
      const tooltipBounds = tooltip.getBoundingClientRect();
      const right = bounds.right + inset;
      const left = right + tooltipBounds.width <= window.innerWidth - inset ? right : bounds.left - tooltipBounds.width - inset;
      setPosition({
        left: Math.max(inset, Math.min(left, window.innerWidth - tooltipBounds.width - inset)),
        top: Math.max(inset, Math.min(bounds.top + (bounds.height - tooltipBounds.height) / 2, window.innerHeight - tooltipBounds.height - inset)),
      });
    }
    positionTooltip();
    window.addEventListener("resize", positionTooltip);
    window.addEventListener("scroll", positionTooltip, true);
    return () => {
      const remaining = (target.getAttribute("aria-describedby") ?? "").split(/\s+/).filter((id) => id && id !== tooltipId);
      if (remaining.length) target.setAttribute("aria-describedby", remaining.join(" "));
      else target.removeAttribute("aria-describedby");
      window.removeEventListener("resize", positionTooltip);
      window.removeEventListener("scroll", positionTooltip, true);
    };
  }, [target, text, tooltipId]);

  if (!target || !text) return null;
  return createPortal(
    <div key={text} ref={tooltipRef} id={tooltipId} className="rm-tooltip" role="tooltip" style={{ "--tooltip-left": `${position.left}px`, "--tooltip-top": `${position.top}px` } as CSSProperties}>{text}</div>,
    document.body,
  );
}
