import { useEffect, useRef, useState, type CSSProperties } from "react";
import { notificationReadingTime } from "../Toast";

export function TimedNotice({ message, onDismiss }: { message: string; onDismiss: () => void }) {
  const [exiting, setExiting] = useState(false);
  const dismissRef = useRef(onDismiss);
  dismissRef.current = onDismiss;
  const duration = notificationReadingTime(message, 5000);

  useEffect(() => {
    setExiting(false);
    const exitTimeout = window.setTimeout(() => setExiting(true), duration);
    const dismissTimeout = window.setTimeout(() => dismissRef.current(), duration + 300);
    return () => {
      window.clearTimeout(exitTimeout);
      window.clearTimeout(dismissTimeout);
    };
  }, [message, duration]);

  return (
    <div className={`account-notice-shell ${exiting ? "is-exiting" : ""}`} style={{ "--notice-duration": `${duration}ms` } as CSSProperties}>
      <p className="account-notice" role="status">
        <span className="notice-timer" aria-hidden="true" />
        {message}
      </p>
    </div>
  );
}
