import { useEffect, type ReactNode } from "react";

type PopupProps = {
  children: ReactNode;
  className: string;
  backdropClassName: string;
  onClose?: () => void;
  closeOnBackdrop?: boolean;
  isClosing?: boolean;
  role?: "dialog" | "alertdialog";
  labelledBy?: string;
  describedBy?: string;
  busy?: boolean;
};

export function Popup({
  children,
  className,
  backdropClassName,
  onClose,
  closeOnBackdrop = false,
  isClosing = false,
  role = "dialog",
  labelledBy,
  describedBy,
  busy = false,
}: PopupProps) {
  useEffect(() => {
    if (!onClose) return;
    function dismissOnEscape(event: KeyboardEvent) {
      if (event.key === "Escape") onClose?.();
    }
    document.addEventListener("keydown", dismissOnEscape);
    return () => document.removeEventListener("keydown", dismissOnEscape);
  }, [onClose]);

  return (
    <div
      className={`${backdropClassName} ${isClosing ? "is-closing" : ""}`.trim()}
      role="presentation"
      onPointerDown={closeOnBackdrop ? () => onClose?.() : undefined}
    >
      <section
        className={`${className} ${isClosing ? "is-closing" : ""}`.trim()}
        role={role}
        aria-modal="true"
        aria-labelledby={labelledBy}
        aria-describedby={describedBy}
        aria-busy={busy}
        onPointerDown={(event) => event.stopPropagation()}
      >
        {children}
      </section>
    </div>
  );
}
