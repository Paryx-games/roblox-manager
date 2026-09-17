import { useLayoutEffect, useState } from "react";
import { createPortal } from "react-dom";
import type { CSSProperties, ReactNode } from "react";

export function ActionMenu({
  children,
  className = "",
}: {
  children: ReactNode;
  className?: string;
}) {
  const [position, setPosition] = useState<CSSProperties>({
    visibility: "hidden",
  });

  useLayoutEffect(() => {
    const updatePosition = () => {
      const trigger =
        document.activeElement?.closest("button") ??
        document.querySelector("[aria-expanded='true']");
      const bounds = trigger?.getBoundingClientRect();
      if (!bounds) return;

      const menuHeight = 240;
      const gap = 4;
      const isAbove = bounds.bottom + gap + menuHeight > window.innerHeight &&
        bounds.top - gap > menuHeight;

      setPosition({
        "--menu-top": `${isAbove ? bounds.top - gap : bounds.bottom + gap}px`,
        "--menu-left": `${Math.max(8, bounds.right - 180)}px`,
        "--menu-transform-y": isAbove ? "-100%" : "0",
        "--menu-max-height": `${Math.max(96, isAbove ? bounds.top - gap - 8 : window.innerHeight - bounds.bottom - gap - 8)}px`,
        visibility: "visible",
      } as CSSProperties);
    };

    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, []);

  return createPortal(
    <div
      className={`account-menu ${className}`.trim()}
      role="menu"
      style={position}
      onPointerDown={(event) => event.stopPropagation()}
    >
      {children}
    </div>,
    document.body,
  );
}
