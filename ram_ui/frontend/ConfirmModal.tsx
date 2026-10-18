import { useId, type ReactNode } from "react";
import { Popup } from "./components/Popup";
import { Icon } from "./components/Icon";

type ConfirmModalProps = {
  title: string;
  message: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  confirmDisabled?: boolean;
  confirmIcon?: string;
  busy?: boolean;
  destructive?: boolean;
};

function ModalIcon({ name }: { name: string }) {
  return (
    <img
      className="account-icon"
      src={`/icons/${name}.svg`}
      alt=""
      aria-hidden="true"
    />
  );
}

export function ConfirmModal({
  title,
  message,
  confirmLabel,
  onConfirm,
  onCancel,
  confirmDisabled = false,
  confirmIcon = "delete",
  busy = false,
  destructive = false,
}: ConfirmModalProps) {
  const titleId = useId();
  const messageId = useId();
  return (
    <Popup
      className="confirm-modal"
      backdropClassName="confirm-modal-backdrop"
      onClose={onCancel}
      closeOnBackdrop
      role="alertdialog"
      labelledBy={titleId}
      describedBy={messageId}
      busy={busy}
    >
        <div className="confirm-modal-header">
          <h2 id={titleId}>{title}</h2>
          <button
            className="icon-button"
            type="button"
            aria-label="Cancel"
            data-tip="Cancel"
            disabled={busy}
            onClick={onCancel}
          >
            <ModalIcon name="close" />
          </button>
        </div>
        <div id={messageId} className="confirm-modal-message">
          {message}
        </div>
        <div className="confirm-modal-actions">
          <button className="account-button" type="button" disabled={busy} onClick={onCancel}>
            <ModalIcon name="close" />
            Cancel
          </button>
          <button
            className={`account-button ${destructive ? "confirm-modal-destructive" : "confirm-modal-danger"}`}
            type="button"
            disabled={confirmDisabled || busy}
            onClick={onConfirm}
          >
            <Icon name={confirmIcon} tone={destructive ? "current-color" : "image"} />
            {confirmLabel}
          </button>
        </div>
    </Popup>
  );
}
