import { AlertTriangle } from "lucide-react";
import { useEffect, useRef } from "react";
import type { ElementRef, KeyboardEvent } from "react";
import type { Messages } from "../../i18n";

interface PinnedDeleteDialogProps {
  compactMode: boolean;
  messages: Messages;
  onCancel: () => void;
  onConfirm: () => void;
}

export function PinnedDeleteDialog({
  compactMode,
  messages,
  onCancel,
  onConfirm,
}: PinnedDeleteDialogProps) {
  const cancelRef = useRef<ElementRef<"button">>(null);

  useEffect(() => {
    cancelRef.current?.focus();
  }, []);

  const handleKeyDown = (event: KeyboardEvent<ElementRef<"div">>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onCancel();
    } else if (event.key === "Tab") {
      const buttons = event.currentTarget.querySelectorAll<
        ElementRef<"button">
      >("button:not(:disabled)");
      const first = buttons[0];
      const last = buttons[buttons.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    }
  };

  return (
    <div className="modal-overlay">
      <div
        aria-describedby="delete-pinned-description"
        aria-labelledby="delete-pinned-title"
        aria-modal="true"
        className="modal-content"
        onKeyDown={handleKeyDown}
        role="dialog"
      >
        <div className="modal-header">
          <AlertTriangle
            aria-hidden="true"
            className="warning-icon"
            size={24}
          />
          <h3 id="delete-pinned-title">
            {messages.deletePinnedConfirmationTitle}
          </h3>
        </div>
        <div className="modal-body" id="delete-pinned-description">
          <p>{messages.deletePinnedConfirmationDescription}</p>
          {compactMode && <p>{messages.deletePinnedCompactDescription}</p>}
          <p className="warning-text">{messages.cannotUndo}</p>
        </div>
        <div className="modal-actions">
          <button
            className="btn btn-secondary"
            onClick={onCancel}
            ref={cancelRef}
            type="button"
          >
            {messages.cancel}
          </button>
          <button className="btn btn-danger" onClick={onConfirm} type="button">
            {messages.deleteItem}
          </button>
        </div>
      </div>
    </div>
  );
}
