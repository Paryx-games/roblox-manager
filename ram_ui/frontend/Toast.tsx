import { useEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./components/Icon";

export type ToastKind = "info" | "success" | "warning" | "error";
export type ToastDuration = "short" | "standard" | "long";

export type ToastItem = {
  id: number;
  title: string;
  message: string;
  kind: ToastKind;
  duration: ToastDuration;
};

const DURATION_MS: Record<ToastDuration, number> = {
  short: 3000,
  standard: 5000,
  long: 8000,
};

function kindIcon(kind: ToastKind) {
  const icons: Record<ToastKind, string> = {
    success: "check",
    info: "info-mark",
    warning: "triangle-alert",
    error: "octagon-alert",
  };
  return icons[kind];
}

export function ToastStack({ children, className = "" }: { children: ReactNode; className?: string }) {
  return createPortal(
    <div className={`toast-stack ${className}`} aria-live="polite" aria-label="Notifications">
      {children}
    </div>,
    document.body,
  );
}

export function Toast({
  item,
  onDismiss,
  placement = "viewport",
}: {
  item: ToastItem;
  onDismiss: () => void;
  placement?: "viewport" | "stack";
}) {
  const [exiting, setExiting] = useState(false);
  const dismissRef = useRef(onDismiss);
  dismissRef.current = onDismiss;

  useEffect(() => {
    setExiting(false);
    const duration = DURATION_MS[item.duration];
    const exitTimeout = window.setTimeout(() => setExiting(true), duration);
    const dismissTimeout = window.setTimeout(
      () => dismissRef.current(),
      duration + 300,
    );
    return () => {
      window.clearTimeout(exitTimeout);
      window.clearTimeout(dismissTimeout);
    };
  }, [item.id]);

  const notification = (
    <div
      className={`toast toast-${item.kind} toast-duration-${item.duration} ${
        exiting ? "is-exiting" : ""
      }`}
      role={item.kind === "error" ? "alert" : "status"}
    >
      <span className="toast-icon">
        <Icon name={kindIcon(item.kind)} tone="current-color" />
      </span>
      <div className="toast-copy">
        <strong>{item.title}</strong>
        <p>{item.message}</p>
      </div>
      <button
        className="toast-dismiss"
        type="button"
        aria-label="Dismiss notification"
        onClick={onDismiss}
      >
        <Icon name="close" tone="current-color" />
      </button>
    </div>
  );
  return placement === "stack" ? notification : <ToastStack>{notification}</ToastStack>;
}
