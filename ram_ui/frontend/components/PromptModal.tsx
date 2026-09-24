import { useState } from "react";
import { Popup } from "./Popup";
import { Icon } from "./Icon";

type PromptModalProps = {
  title: string;
  value?: string;
  onSubmit: (value: string) => void;
  onCancel: () => void;
};

export function PromptModal({
  title,
  value = "",
  onSubmit,
  onCancel,
}: PromptModalProps) {
  const [inputValue, setInputValue] = useState(value);

  return (
    <Popup
      className="confirm-modal"
      backdropClassName="confirm-modal-backdrop"
      onClose={onCancel}
      closeOnBackdrop
      labelledBy="prompt-modal-title"
    >
      <div className="confirm-modal-header">
        <h2 id="prompt-modal-title">{title}</h2>
        <button className="icon-button" type="button" aria-label="Cancel" onClick={onCancel}>
          <Icon name="close" />
        </button>
      </div>
      <form
        className="prompt-modal-form"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit(inputValue);
        }}
      >
        <label htmlFor="prompt-modal-input">{title}</label>
        <input
          id="prompt-modal-input"
          autoFocus
          maxLength={80}
          value={inputValue}
          onChange={(event) => setInputValue(event.target.value)}
        />
        <div className="confirm-modal-actions">
          <button className="account-button" type="button" onClick={onCancel}>Cancel</button>
          <button className="account-button primary" type="submit" disabled={!inputValue.trim()}>Save</button>
        </div>
      </form>
    </Popup>
  );
}
