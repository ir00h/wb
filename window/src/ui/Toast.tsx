import type { MouseEvent } from "react";
import { dismissNotice, quitWithoutSaving, useWindow } from "../state/store";

function keepFocus(event: MouseEvent) {
  event.preventDefault();
}

export function Toast() {
  const notice = useWindow((state) => state.notice);
  if (notice === null) {
    return null;
  }
  return (
    <div className="toast" role="status">
      <span>{notice.text}</span>
      {notice.kind === "notQuit" && (
        <button
          type="button"
          data-quit-without-saving
          onMouseDown={keepFocus}
          onClick={quitWithoutSaving}
        >
          Quit without saving
        </button>
      )}
      <button type="button" onClick={dismissNotice}>
        Dismiss
      </button>
    </div>
  );
}
