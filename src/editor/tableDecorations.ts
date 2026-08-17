import { StateField, type EditorState, type Text } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, WidgetType } from "@codemirror/view";

// GFM-style pipe tables aren't part of CodeMirror's default markdown grammar
// (@lezer/markdown needs an extra extension for that), and even parsed,
// aligning pipes into a real grid needs actual <table> layout — inline mark
// decorations (the approach the rest of markdownDecorations.ts uses) can't
// do that. So this scans the raw text for table blocks directly and swaps
// the whole block for a rendered <table>, reverting to plain source text
// while the cursor is inside it so it stays editable.

interface TableBlock {
  from: number;
  to: number;
  headerCells: string[];
  bodyRows: string[][];
}

function parseCells(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|")) s = s.slice(0, -1);
  return s.split("|").map((c) => c.trim());
}

function isSeparatorRow(line: string): boolean {
  const cells = parseCells(line);
  return cells.length > 0 && cells.every((c) => /^:?-+:?$/.test(c));
}

// Minimal inline-markdown rendering for cell content (bold/italic/code) —
// not the full grammar, just enough that emphasis inside a table (a common
// pattern for highlighting the row's subject) actually renders instead of
// showing literal asterisks.
const INLINE_RE = /\*\*(.+?)\*\*|__(.+?)__|\*(.+?)\*|_(.+?)_|`(.+?)`/g;

function renderInlineMarkdown(text: string): DocumentFragment {
  const frag = document.createDocumentFragment();
  let lastIndex = 0;
  INLINE_RE.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = INLINE_RE.exec(text))) {
    if (match.index > lastIndex) frag.appendChild(document.createTextNode(text.slice(lastIndex, match.index)));
    if (match[1] !== undefined || match[2] !== undefined) {
      const strong = document.createElement("strong");
      strong.textContent = match[1] ?? match[2];
      frag.appendChild(strong);
    } else if (match[3] !== undefined || match[4] !== undefined) {
      const em = document.createElement("em");
      em.textContent = match[3] ?? match[4];
      frag.appendChild(em);
    } else if (match[5] !== undefined) {
      const code = document.createElement("code");
      code.textContent = match[5];
      frag.appendChild(code);
    }
    lastIndex = INLINE_RE.lastIndex;
  }
  if (lastIndex < text.length) frag.appendChild(document.createTextNode(text.slice(lastIndex)));
  return frag;
}

function findTables(doc: Text): TableBlock[] {
  const blocks: TableBlock[] = [];
  const totalLines = doc.lines;
  let lineNo = 1;
  while (lineNo < totalLines) {
    const line = doc.line(lineNo);
    const nextLine = doc.line(lineNo + 1);
    if (line.text.includes("|") && isSeparatorRow(nextLine.text)) {
      const headerCells = parseCells(line.text);
      const bodyRows: string[][] = [];
      let endLineNo = lineNo + 1;
      let cursor = lineNo + 2;
      while (cursor <= totalLines) {
        const l = doc.line(cursor);
        if (l.text.trim() === "" || !l.text.includes("|")) break;
        bodyRows.push(parseCells(l.text));
        endLineNo = cursor;
        cursor++;
      }
      blocks.push({ from: line.from, to: doc.line(endLineNo).to, headerCells, bodyRows });
      lineNo = endLineNo + 1;
    } else {
      lineNo++;
    }
  }
  return blocks;
}

class TableWidget extends WidgetType {
  constructor(
    private headerCells: string[],
    private bodyRows: string[][],
  ) {
    super();
  }

  eq(other: TableWidget) {
    return (
      JSON.stringify(this.headerCells) === JSON.stringify(other.headerCells) &&
      JSON.stringify(this.bodyRows) === JSON.stringify(other.bodyRows)
    );
  }

  toDOM() {
    const wrap = document.createElement("div");
    wrap.className = "cm-md-table-wrap";
    const table = document.createElement("table");
    table.className = "cm-md-table";

    const thead = document.createElement("thead");
    const headRow = document.createElement("tr");
    for (const cell of this.headerCells) {
      const th = document.createElement("th");
      th.appendChild(renderInlineMarkdown(cell));
      headRow.appendChild(th);
    }
    thead.appendChild(headRow);
    table.appendChild(thead);

    const tbody = document.createElement("tbody");
    for (const row of this.bodyRows) {
      const tr = document.createElement("tr");
      for (let i = 0; i < this.headerCells.length; i++) {
        const td = document.createElement("td");
        td.appendChild(renderInlineMarkdown(row[i] ?? ""));
        tr.appendChild(td);
      }
      tbody.appendChild(tr);
    }
    table.appendChild(tbody);

    wrap.appendChild(table);
    return wrap;
  }

  // Don't eat clicks — let CM6 place the cursor near the widget so clicking
  // a rendered table drops you back into its editable source.
  ignoreEvent() {
    return false;
  }
}

function buildDecorations(state: EditorState): DecorationSet {
  const blocks = findTables(state.doc);
  const sel = state.selection.main;
  const decos = [];
  for (const block of blocks) {
    const cursorInside = sel.from <= block.to && sel.to >= block.from;
    if (cursorInside) continue;
    decos.push(
      Decoration.replace({
        widget: new TableWidget(block.headerCells, block.bodyRows),
        block: true,
      }).range(block.from, block.to),
    );
  }
  return Decoration.set(decos, true);
}

// Block decorations can only be supplied by a StateField (a ViewPlugin's
// `decorations` provider is restricted to non-block decorations by CM6) —
// state, not viewport, drives this anyway since tables are found by scanning
// the whole document rather than just the visible range.
const tableField = StateField.define<DecorationSet>({
  create(state) {
    return buildDecorations(state);
  },
  update(value, tr) {
    if (tr.docChanged || tr.selection) return buildDecorations(tr.state);
    return value.map(tr.changes);
  },
  provide: (f) => EditorView.decorations.from(f),
});

export function tableDecorations() {
  return tableField;
}
