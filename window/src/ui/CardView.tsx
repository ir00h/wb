import { type CSSProperties, memo, useCallback, useRef } from "react";
import type { NoteCard } from "../bindings/NoteCard";
import type { TagHue } from "../bindings/TagHue";
import type { CardPlacement } from "../domain/layout";
import { cardLook } from "../domain/look";
import { measured, useWindow } from "../state/store";
import { CardEditor } from "./CardEditor";
import { Markdown } from "./Markdown";

type Props = {
  readonly card: NoteCard;
  readonly placement: CardPlacement;
  readonly hues: readonly TagHue[];
};

export const CardView = memo(function CardView({ card, placement, hues }: Props) {
  const editing = useWindow(
    (state) => state.interaction.kind === "editing" && state.interaction.note === card.id,
  );
  const observer = useRef<ResizeObserver | null>(null);
  const measure = useCallback(
    (element: HTMLDivElement | null) => {
      observer.current?.disconnect();
      observer.current = null;
      if (element === null) {
        return;
      }
      const watching = new ResizeObserver(() => measured(card.id, element.offsetHeight));
      watching.observe(element);
      observer.current = watching;
    },
    [card.id],
  );
  const look = cardLook(card, hues);
  const style: CSSProperties = {
    left: placement.rect.x,
    top: placement.rect.y,
    width: placement.rect.width,
    ...(look.kind === "tinted" ? { ["--hue" as string]: look.hue } : {}),
  };
  const classes = ["card", look.kind === "tinted" ? "tinted" : "", editing ? "editing" : ""]
    .filter((name) => name !== "")
    .join(" ");
  return (
    <div ref={measure} className={classes} data-card={card.id} style={style}>
      {card.tags.length > 0 && (
        <div className="card-tag">
          <span className="card-tags">
            {card.tags.map((tag) => (
              <b
                key={tag}
                className={look.kind === "tinted" && look.tag === tag ? "coloured" : undefined}
              >
                {tag}
              </b>
            ))}
          </span>
        </div>
      )}
      {card.title !== "" && <div className="card-title">{card.title}</div>}
      {editing ? <CardEditor note={card.id} /> : card.body !== "" && <Markdown text={card.body} />}
    </div>
  );
});
