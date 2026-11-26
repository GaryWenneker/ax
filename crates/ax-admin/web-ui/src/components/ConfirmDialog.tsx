import ModalShell from '@cc/components/ModalShell';

interface Props {
  title: string;
  message: string;
  confirmLabel: string;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Asks before something that changes or removes data. Escape and Cancel back out. */
export default function ConfirmDialog({ title, message, confirmLabel, onConfirm, onCancel }: Props) {
  return (
    <ModalShell
      title={title}
      onClose={onCancel}
      footer={
        <div className="admin-actions admin-actions--end">
          <button type="button" className="btn btn-subtle" onClick={onCancel}>Cancel</button>
          <button type="button" className="btn btn-danger-subtle" onClick={onConfirm}>{confirmLabel}</button>
        </div>
      }
    >
      <p className="admin-confirm-text">{message}</p>
    </ModalShell>
  );
}
