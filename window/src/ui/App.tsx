import { useEffect } from "react";
import { PRODUCT_NAME, windowTitle } from "../domain/product";
import { useApp } from "../state/app";
import "./app.css";

export function App() {
  const board = useApp((state) => state.board);
  const inDesktopShell = useApp((state) => state.inDesktopShell);

  useEffect(() => {
    document.title = windowTitle(board);
  }, [board]);

  return (
    <main className="placeholder">
      <h1>{PRODUCT_NAME}</h1>
      <p>{board === null ? "No board open" : board}</p>
      <p className="host">{inDesktopShell ? "Desktop shell" : "Browser preview"}</p>
    </main>
  );
}
