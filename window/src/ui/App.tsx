import { type CSSProperties, useEffect } from "react";
import { TITLE_BAR } from "../bindings/Calls";
import { unreachable } from "../domain/exhaustive";
import { PRODUCT_NAME, windowTitle } from "../domain/product";
import { start, useWindow } from "../state/store";
import { Canvas } from "./Canvas";
import { Toast } from "./Toast";
import "./styles/tokens.css";
import "./styles/base.css";
import "./styles/chrome.css";
import "./styles/canvas.css";
import "./styles/markdown.css";

const CHROME: CSSProperties = {
  ["--bar-height" as string]: `${TITLE_BAR.height}px`,
  ["--content-inset" as string]: `${TITLE_BAR.contentInset}px`,
};

function Body() {
  const board = useWindow((state) => state.board);
  switch (board.kind) {
    case "opening":
      return <p className="notice">Opening…</p>;
    case "preview":
      return <p className="notice">This page shows a board only inside the desktop app.</p>;
    case "failed":
      return <p className="notice">{board.message}</p>;
    case "open":
      return <Canvas snapshot={board.snapshot} problem={board.problem} />;
    default:
      return unreachable(board);
  }
}

function TitleBar() {
  const board = useWindow((state) => state.board);
  const name = board.kind === "open" ? board.snapshot.board.name : PRODUCT_NAME;
  const count = board.kind === "open" ? board.snapshot.notes.length : null;
  useEffect(() => {
    document.title = windowTitle(board.kind === "open" ? board.snapshot.board.name : null);
  }, [board]);
  return (
    <header className="title-bar" data-tauri-drag-region>
      <h1 data-tauri-drag-region>{name}</h1>
      {count !== null && (
        <span className="card-count" data-tauri-drag-region>
          {count === 1 ? "1 card" : `${count} cards`}
        </span>
      )}
    </header>
  );
}

export function App() {
  useEffect(() => {
    void start();
  }, []);
  return (
    <div className="app" style={CHROME}>
      <TitleBar />
      <Body />
      <Toast />
    </div>
  );
}
