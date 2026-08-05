import { syntaxTree } from "@codemirror/language";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate } from "@codemirror/view";

// Live-preview styling driven by CodeMirror's own markdown syntax tree
// (@codemirror/lang-markdown, lezer-markdown grammar) rather than a
// hand-rolled regex — the wikilink decorations use regex because wikilinks
// aren't part of standard markdown grammar, but headers/emphasis/code/quotes
// already come pre-parsed, so reusing that tree is both simpler and correct
// on edge cases (nesting, escaping) a regex would get wrong.

const HEADER_CLASS: Record<string, string> = {
  ATXHeading1: "cm-md-h1",
  ATXHeading2: "cm-md-h2",
  ATXHeading3: "cm-md-h3",
  ATXHeading4: "cm-md-h4",
  ATXHeading5: "cm-md-h5",
  ATXHeading6: "cm-md-h6",
};

function buildDecorations(view: EditorView): DecorationSet {
  const positions: [number, number, ReturnType<typeof Decoration.mark>][] = [];

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(view.state).iterate({
      from,
      to,
      enter: (node) => {
        const headerClass = HEADER_CLASS[node.name];
        if (headerClass) {
          positions.push([node.from, node.to, Decoration.mark({ class: `cm-md-header ${headerClass}` })]);
        } else if (node.name === "StrongEmphasis") {
          positions.push([node.from, node.to, Decoration.mark({ class: "cm-md-strong" })]);
        } else if (node.name === "Emphasis") {
          positions.push([node.from, node.to, Decoration.mark({ class: "cm-md-em" })]);
        } else if (node.name === "InlineCode") {
          positions.push([node.from, node.to, Decoration.mark({ class: "cm-md-code" })]);
        } else if (node.name === "Blockquote") {
          positions.push([node.from, node.to, Decoration.mark({ class: "cm-md-quote" })]);
        } else if (node.name === "HeaderMark" || node.name === "EmphasisMark" || node.name === "CodeMark" || node.name === "QuoteMark") {
          positions.push([node.from, node.to, Decoration.mark({ class: "cm-md-mark" })]);
        }
      },
    });
  }

  return Decoration.set(
    positions.map(([from, to, deco]) => deco.range(from, to)),
    true,
  );
}

export function markdownDecorations() {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = buildDecorations(view);
      }
      update(update: ViewUpdate) {
        if (update.docChanged || update.viewportChanged) {
          this.decorations = buildDecorations(update.view);
        }
      }
    },
    { decorations: (v) => v.decorations },
  );
}
