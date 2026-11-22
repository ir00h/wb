import { memo } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

const PLUGINS = [remarkGfm];

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="md card-body">
      <ReactMarkdown remarkPlugins={PLUGINS} skipHtml>
        {text}
      </ReactMarkdown>
    </div>
  );
});
