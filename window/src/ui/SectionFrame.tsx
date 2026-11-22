import type { CSSProperties } from "react";
import type { FramePlacement } from "../domain/layout";

const MAX_DEPTH = 4;
const UNCOLOURED_HUE = 210;

export function SectionFrame({ frame }: { frame: FramePlacement }) {
  const { section, rect, depth } = frame;
  const style: CSSProperties = {
    left: rect.x,
    top: rect.y,
    width: rect.width,
    height: rect.height,
    zIndex: depth,
    ["--depth" as string]: Math.min(depth, MAX_DEPTH),
    ["--hue" as string]: section.hue ?? UNCOLOURED_HUE,
  };
  return (
    <div
      className={depth > 0 ? "section nested" : "section"}
      data-section-body={section.id}
      style={style}
    >
      <div className="section-header" data-section={section.id}>
        <h2 className="section-title">{section.name}</h2>
        {section.note !== "" && <p className="section-note">{section.note}</p>}
      </div>
    </div>
  );
}
