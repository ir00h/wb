import { dismissNotice, useWindow } from "../state/store";

export function Toast() {
  const notice = useWindow((state) => state.notice);
  if (notice === null) {
    return null;
  }
  return (
    <div className="toast" role="status">
      <span>{notice}</span>
      <button type="button" onClick={dismissNotice}>
        Dismiss
      </button>
    </div>
  );
}
