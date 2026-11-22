import { type MouseEvent, useMemo, type WheelEvent } from "react";
import type { BoardSnapshot } from "../bindings/BoardSnapshot";
import { layout } from "../domain/layout";
import { input, pan, useWindow } from "../state/store";
import { CardView } from "./CardView";
import { SectionFrame } from "./SectionFrame";

const DOT_STEP = 26;
const DOT_MARGIN = 80;

function dotShift(offset: number): number {
  return (((offset + DOT_MARGIN) % DOT_STEP) + DOT_STEP) % DOT_STEP;
}

function Strays({ names }: { names: readonly string[] }) {
  if (names.length === 0) {
    return null;
  }
  return (
    <aside className="strays" data-strays>
      {names.length === 1
        ? "A file without an id is not shown: "
        : "Files without an id are not shown: "}
      {names.map((name, index) => (
        <span key={name}>
          {index > 0 && ", "}
          <b>{name}</b>
        </span>
      ))}
    </aside>
  );
}

export function Canvas({ snapshot }: { snapshot: BoardSnapshot }) {
  const heights = useWindow((state) => state.heights);
  const view = useWindow((state) => state.view);
  const placed = useMemo(() => layout(snapshot, heights), [snapshot, heights]);
  const cards = useMemo(() => new Map(snapshot.notes.map((note) => [note.id, note])), [snapshot]);

  const onWheel = (event: WheelEvent) => pan(event.deltaX, event.deltaY);
  const onDoubleClick = (event: MouseEvent) => {
    const target = event.target instanceof Element ? event.target.closest("[data-card]") : null;
    const id = target?.getAttribute("data-card");
    const card = snapshot.notes.find((note) => note.id === id);
    if (card !== undefined) {
      input({ type: "doubleClicked", note: card.id });
    }
  };

  return (
    <main className="canvas" onWheel={onWheel} onDoubleClick={onDoubleClick}>
      <div
        className="dots"
        style={{ transform: `translate(${dotShift(view.x)}px, ${dotShift(view.y)}px)` }}
      />
      <div className="world" style={{ transform: `translate(${view.x}px, ${view.y}px)` }}>
        {placed.frames.map((frame) => (
          <SectionFrame key={frame.section.id} frame={frame} />
        ))}
        {placed.cards.map((placement) => {
          const card = cards.get(placement.note);
          return card === undefined ? null : (
            <CardView key={card.id} card={card} placement={placement} hues={snapshot.tags} />
          );
        })}
      </div>
      <Strays names={snapshot.strays} />
    </main>
  );
}
