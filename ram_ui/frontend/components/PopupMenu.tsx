import type { CSSProperties, KeyboardEventHandler, ReactNode } from "react";

type PopupMenuProps = {
  children: ReactNode;
  className: string;
  style?: CSSProperties;
  menuRef?: (element: HTMLDivElement | null) => void;
  onKeyDown?: KeyboardEventHandler<HTMLDivElement>;
};

export function PopupMenu({ children, className, style, menuRef, onKeyDown }: PopupMenuProps) {
  return (
    <div
      className={`popup-menu ${className}`.trim()}
      ref={menuRef}
      style={style}
      role="menu"
      onPointerDown={(event) => event.stopPropagation()}
      onKeyDown={onKeyDown}
    >
      {children}
    </div>
  );
}
