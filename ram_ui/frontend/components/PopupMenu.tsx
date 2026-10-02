import { useEffect, useRef, type CSSProperties, type KeyboardEventHandler, type ReactNode } from "react";
import { useOverlayDismiss } from "../hooks/useOverlayDismiss";

type PopupMenuProps = {
  children: ReactNode;
  className: string;
  style?: CSSProperties;
  menuRef?: (element: HTMLDivElement | null) => void;
  onKeyDown?: KeyboardEventHandler<HTMLDivElement>;
  onClose: () => void;
};

export function PopupMenu({ children, className, style, menuRef, onKeyDown, onClose }: PopupMenuProps) {
  const ref = useRef<HTMLDivElement | null>(null);
  const previousFocus = useRef<Element | null>(null);
  useOverlayDismiss(ref, () => {
    onClose();
    if (previousFocus.current instanceof HTMLElement && previousFocus.current.isConnected) previousFocus.current.focus();
  });

  useEffect(() => {
    previousFocus.current = document.activeElement;
    ref.current?.querySelector<HTMLElement>('button:not([disabled]), [href], [tabindex="0"]')?.focus();
    const menu = ref.current;
    const trigger = previousFocus.current;
    return () => {
      if (menu?.contains(document.activeElement) && trigger instanceof HTMLElement && trigger.isConnected) trigger.focus();
    };
  }, []);

  return (
    <div
      className={`popup-menu ${className}`.trim()}
      ref={(element) => {
        ref.current = element;
        menuRef?.(element);
      }}
      data-rm-overlay
      style={style}
      role="menu"
      onPointerDown={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (event.defaultPrevented) return;
        if (event.key === "Tab") {
          onClose();
          return;
        }
        const options = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('button:not([disabled]), [href], [tabindex="0"]'))
          .filter((element) => element.getClientRects().length > 0);
        if (!options.length) return;
        const index = options.indexOf(document.activeElement as HTMLElement);
        let next: number;
        if (event.key === "ArrowDown") next = (index + 1) % options.length;
        else if (event.key === "ArrowUp") next = (index - 1 + options.length) % options.length;
        else if (event.key === "Home") next = 0;
        else if (event.key === "End") next = options.length - 1;
        else return;
        event.preventDefault();
        options[next].focus();
      }}
    >
      {children}
    </div>
  );
}
