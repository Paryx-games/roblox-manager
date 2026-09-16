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

      setPosition({
        "--menu-top": `${bounds.bottom + 4}px`,
        "--menu-left": `${Math.max(8, bounds.right - 180)}px`,
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
