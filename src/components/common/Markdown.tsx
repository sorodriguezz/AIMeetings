import { useMemo } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";

marked.setOptions({ gfm: true, breaks: false });

/** Markdown rendered and sanitised — the AI's answers and the meeting's notes. */
export function Markdown({ source, className = "" }: { source: string; className?: string }) {
  const html = useMemo(() => DOMPurify.sanitize(marked.parse(source, { async: false }) as string), [source]);
  return <div className={`cf-markdown ${className}`} dangerouslySetInnerHTML={{ __html: html }} />;
}
