import { useEffect, useRef, useState } from "react";

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

function Icon({ name }: { name: string }) {
  return (
    <img
      className="account-icon"
      src={`/icons/${name}.svg`}
      alt=""
      aria-hidden="true"
    />
  );
}

function kindIcon(kind: ToastKind) {
  return kind === "warning" || kind === "error" ? "warning" : "update";
}

export function Toast({
  item,
  onDismiss,
}: {
  item: ToastItem;
  onDismiss: () => void;
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

  return (
    <div
      className={`toast toast-${item.kind} toast-duration-${item.duration} ${
        exiting ? "is-exiting" : ""
      }`}
      role={item.kind === "error" ? "alert" : "status"}
    >
      <span className="toast-icon">
        <Icon name={kindIcon(item.kind)} />
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
        <Icon name="close" />
      </button>
      <span className="toast-timer" aria-hidden="true" />
    </div>
  );
}
