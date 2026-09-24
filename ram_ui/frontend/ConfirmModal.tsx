import type { ReactNode } from "react";
import { Popup } from "./components/Popup";

type ConfirmModalProps = {
  title: string;
  message: ReactNode;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
  confirmDisabled?: boolean;
  confirmIcon?: string;
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
}: ConfirmModalProps) {
  return (
    <Popup
      className="confirm-modal"
      backdropClassName="confirm-modal-backdrop"
      onClose={onCancel}
      closeOnBackdrop
      role="alertdialog"
      labelledBy="confirm-modal-title"
      describedBy="confirm-modal-message"
    >
        <div className="confirm-modal-header">
          <h2 id="confirm-modal-title">{title}</h2>
          <button
            className="icon-button"
            type="button"
            aria-label="Cancel"
            data-tip="Cancel"
            onClick={onCancel}
          >
            <ModalIcon name="close" />
          </button>
        </div>
        <div id="confirm-modal-message" className="confirm-modal-message">
          {message}
        </div>
        <div className="confirm-modal-actions">
          <button className="account-button" type="button" onClick={onCancel}>
            <ModalIcon name="close" />
            No
          </button>
          <button
            className="account-button confirm-modal-danger"
            type="button"
            disabled={confirmDisabled}
            onClick={onConfirm}
          >
            <ModalIcon name={confirmIcon} />
            {confirmLabel}
          </button>
        </div>
    </Popup>
  );
}
