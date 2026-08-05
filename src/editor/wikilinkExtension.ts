import { RangeSetBuilder } from "@codemirror/state";
import { Decoration, DecorationSet, EditorView, ViewPlugin, ViewUpdate } from "@codemirror/view";
import { autocompletion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { listNodes } from "../api/nodes";
import { searchNodes } from "../api/search";

// Matches [[Title]], [[Title|Alias]], [[Title#Heading]], ![[embed]]. This is a
// looser regex than the backend's hand-rolled tokenizer (domain/links.rs) —
// it only drives editor decoration/navigation, not actual link resolution,
// which stays server-side and authoritative.
const WIKILINK_RE = /!?\[\[([^\]\n|#]+)(#[^\]\n|]*)?(\|[^\]\n]*)?\]\]/g;

function findLinkAt(doc: string, pos: number): { start: number; end: number; target: string } | null {
  WIKILINK_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = WIKILINK_RE.exec(doc))) {
    const start = m.index;
    const end = start + m[0].length;
    if (pos >= start && pos <= end) {
      return { start, end, target: m[1].trim() };
    }
  }
  return null;
}

function buildDecorations(view: EditorView): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  const doc = view.state.doc.toString();
  const mark = Decoration.mark({ class: "cm-wikilink" });
  WIKILINK_RE.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = WIKILINK_RE.exec(doc))) {
    builder.add(m.index, m.index + m[0].length, mark);
  }
  return builder.finish();
}

export function wikilinkDecorations() {
  return ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = buildDecorations(view);
      }
      update(update: ViewUpdate) {
        if (update.docChanged) {
          this.decorations = buildDecorations(update.view);
        }
      }
    },
    { decorations: (v) => v.decorations },
  );
}

// Ctrl/Cmd+click a wikilink to navigate — plain click stays reserved for
// normal cursor placement/text editing.
export function wikilinkClickNavigation(onNavigate: (title: string) => void) {
  return EditorView.domEventHandlers({
    mousedown(event, view) {
      if (!event.ctrlKey && !event.metaKey) return false;
      const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
      if (pos == null) return false;
      const link = findLinkAt(view.state.doc.toString(), pos);
      if (!link) return false;
      onNavigate(link.target);
      event.preventDefault();
      return true;
    },
  });
}

async function wikilinkCompletionSource(context: CompletionContext): Promise<CompletionResult | null> {
  const before = context.matchBefore(/\[\[[^\]\n]*/);
  if (!before) return null;
  const query = before.text.slice(2);

  const results = query.length === 0 ? await listNodes({ limit: 8 }) : await searchNodes(query, { limit: 8 });
  const titles = "items" in results ? results.items.map((item) => item.title) : [];

  return {
    from: before.from + 2,
    options: titles.map((title) => ({ label: title, apply: `${title}]]`, type: "text" })),
    validFor: /^[^\]\n]*$/,
  };
}

export function wikilinkAutocomplete() {
  return autocompletion({ override: [wikilinkCompletionSource] });
}
