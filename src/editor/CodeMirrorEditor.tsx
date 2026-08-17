import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { EditorSelection, EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { wikilinkAutocomplete, wikilinkClickNavigation, wikilinkDecorations } from "./wikilinkExtension";
import { markdownDecorations } from "./markdownDecorations";
import { tableDecorations } from "./tableDecorations";
import { aliveTyping } from "./aliveTyping";

interface CodeMirrorEditorProps {
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  onNavigateToTitle: (title: string) => void;
}

export type FormatKind = "bold" | "italic" | "list" | "quote" | "link";

export interface CodeMirrorEditorHandle {
  applyFormat: (kind: FormatKind) => void;
}

export const CodeMirrorEditor = forwardRef<CodeMirrorEditorHandle, CodeMirrorEditorProps>(
  function CodeMirrorEditor({ value, onChange, onBlur, onNavigateToTitle }, ref) {
    const containerRef = useRef<HTMLDivElement | null>(null);
    const viewRef = useRef<EditorView | null>(null);

    // Kept in refs so the extensions (built once on mount) always call the
    // latest callback without needing to tear down/rebuild the editor view.
    const onChangeRef = useRef(onChange);
    onChangeRef.current = onChange;
    const onBlurRef = useRef(onBlur);
    onBlurRef.current = onBlur;
    const onNavigateRef = useRef(onNavigateToTitle);
    onNavigateRef.current = onNavigateToTitle;

    useImperativeHandle(
      ref,
      () => ({
        applyFormat: (kind) => {
          const view = viewRef.current;
          if (!view) return;

          if (kind === "bold" || kind === "italic" || kind === "link") {
            const marker = kind === "bold" ? "**" : kind === "italic" ? "_" : null;
            view.dispatch(
              view.state.changeByRange((range) => {
                const text = view.state.sliceDoc(range.from, range.to);
                const insert = kind === "link" ? `[${text}]()` : `${marker}${text}${marker}`;
                let selFrom: number;
                let selTo: number;
                if (kind === "link") {
                  // cursor lands inside the empty parens, ready for a URL
                  selFrom = selTo = range.from + insert.length - 1;
                } else if (text) {
                  selFrom = range.from + marker!.length;
                  selTo = selFrom + text.length;
                } else {
                  selFrom = selTo = range.from + marker!.length;
                }
                return {
                  changes: { from: range.from, to: range.to, insert },
                  range: EditorSelection.range(selFrom, selTo),
                };
              }),
            );
          } else {
            // list / quote: prefix every line the selection touches, skipping
            // lines that already have the prefix (no toggle/un-wrap — v1).
            const prefix = kind === "list" ? "- " : "> ";
            view.dispatch(
              view.state.changeByRange((range) => {
                const startLine = view.state.doc.lineAt(range.from);
                const endLine = view.state.doc.lineAt(range.to);
                const changes: { from: number; insert: string }[] = [];
                let shiftForFrom = 0;
                for (let n = startLine.number; n <= endLine.number; n++) {
                  const line = view.state.doc.line(n);
                  if (!line.text.startsWith(prefix)) {
                    changes.push({ from: line.from, insert: prefix });
                    if (n === startLine.number) shiftForFrom = prefix.length;
                  }
                }
                const totalShift = changes.length * prefix.length;
                return {
                  changes,
                  range: EditorSelection.range(range.from + shiftForFrom, range.to + totalShift),
                };
              }),
            );
          }
          view.focus();
        },
      }),
      [],
    );

    useEffect(() => {
      if (!containerRef.current) return;

      const state = EditorState.create({
        doc: value,
        extensions: [
          lineNumbers(),
          history(),
          highlightActiveLine(),
          // Explicit, not CM6's default-if-omitted fallback: gives us
          // .cm-cursor/.cm-cursorLayer as real styleable elements with a
          // CSS-animation-driven blink, which the "alive cursor" theming
          // below depends on.
          drawSelection({ cursorBlinkRate: 1000 }),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          markdown(),
          markdownDecorations(),
          tableDecorations(),
          aliveTyping(),
          wikilinkDecorations(),
          wikilinkClickNavigation((title) => onNavigateRef.current(title)),
          wikilinkAutocomplete(),
          EditorView.lineWrapping,
          EditorView.updateListener.of((update) => {
            if (update.docChanged) {
              onChangeRef.current(update.state.doc.toString());
            }
          }),
          EditorView.domEventHandlers({
            blur: () => {
              onBlurRef.current?.();
              return false;
            },
          }),
          EditorView.theme({
            "&": { height: "100%", fontSize: "14px", backgroundColor: "transparent" },
            ".cm-scroller": { overflow: "auto", fontFamily: "ui-monospace, SFMono-Regular, monospace" },
            ".cm-wikilink": { color: "var(--link-color, #5b8def)", cursor: "pointer" },
            // Grow into place rather than snap: fires the moment `# `/`**`/etc.
            // gets recognized and the mark class lands, since CM6 reuses the
            // same DOM span across that edit (only the class list changes).
            ".cm-md-header": {
              fontFamily: "ui-sans-serif, system-ui, sans-serif",
              fontWeight: "700",
              display: "inline-block",
              transition: "font-size 180ms cubic-bezier(0.2, 0.8, 0.2, 1), opacity 180ms ease",
            },
            ".cm-md-h1": { fontSize: "1.6em" },
            ".cm-md-h2": { fontSize: "1.4em" },
            ".cm-md-h3": { fontSize: "1.25em" },
            ".cm-md-h4": { fontSize: "1.12em" },
            ".cm-md-h5": { fontSize: "1em" },
            ".cm-md-h6": { fontSize: "0.92em", opacity: "0.85" },
            ".cm-md-strong": { fontWeight: "700", transition: "font-weight 150ms ease" },
            ".cm-md-em": { fontStyle: "italic" },
            ".cm-md-code": {
              fontFamily: "ui-monospace, SFMono-Regular, monospace",
              background: "color-mix(in srgb, currentColor 10%, transparent)",
              borderRadius: "3px",
              padding: "0 3px",
              transition: "background 150ms ease",
            },
            ".cm-md-quote": { color: "var(--muted, #767676)", fontStyle: "italic" },
            ".cm-md-mark": { opacity: "0.4", transition: "opacity 150ms ease" },
            ".cm-md-table-wrap": {
              overflowX: "auto", margin: "4px 0", cursor: "text",
            },
            ".cm-md-table": {
              borderCollapse: "collapse", fontFamily: "ui-sans-serif, system-ui, sans-serif",
              fontSize: "13px", minWidth: "100%",
            },
            ".cm-md-table th, .cm-md-table td": {
              border: "1px solid var(--color-divider, #3a3d4d)",
              padding: "6px 10px", textAlign: "left", verticalAlign: "top",
            },
            ".cm-md-table th": {
              background: "color-mix(in srgb, currentColor 8%, transparent)",
              fontWeight: "600",
            },
            ".cm-md-table tbody tr:nth-child(even)": {
              background: "color-mix(in srgb, currentColor 3%, transparent)",
            },
            // Freshly-typed text pops in rather than just appearing —
            // aliveTyping.ts marks the exact inserted range for ~250ms.
            ".cm-just-typed": {
              display: "inline-block",
              animation: "cm-char-in 220ms cubic-bezier(0.2, 0.8, 0.2, 1)",
            },
            "@keyframes cm-char-in": {
              from: { opacity: "0.25", transform: "translateY(2px) scale(0.97)" },
              to: { opacity: "1", transform: "translateY(0) scale(1)" },
            },
            // Cursor: CSS-animation blink (from drawSelection) restyled to a
            // smooth fade rather than a hard on/off flicker, plus a brief
            // "pulse" glow on every keystroke (insert or delete) so it feels
            // responsive rather than just sitting there.
            ".cm-cursor-primary": {
              borderLeftColor: "var(--color-accent, var(--link-color, #9184d9))",
              borderLeftWidth: "2px",
              transition: "transform 90ms cubic-bezier(0.2, 0.8, 0.2, 1), box-shadow 90ms ease",
              // Overrides drawSelection's default hard-flicker steps() blink
              // with a smooth fade — same rate, softer feel.
              animation: "cm-cursor-blink 1.1s ease-in-out infinite",
            },
            "@keyframes cm-cursor-blink": {
              "0%, 100%": { opacity: "1" },
              "50%": { opacity: "0.15" },
            },
            ".cm-typing-pulse .cm-cursor-primary": {
              transform: "scaleY(1.1)",
              boxShadow: "0 0 7px 0 var(--color-accent, var(--link-color, #9184d9))",
              opacity: "1",
              animation: "none",
            },
            ".cm-gutters": {
              backgroundColor: "transparent",
              color: "var(--muted, #767676)",
              border: "none",
            },
            ".cm-activeLineGutter": {
              backgroundColor: "color-mix(in srgb, currentColor 8%, transparent)",
              color: "inherit",
            },
            ".cm-activeLine": {
              backgroundColor: "color-mix(in srgb, currentColor 5%, transparent)",
            },
          }),
        ],
      });

      const view = new EditorView({ state, parent: containerRef.current });
      viewRef.current = view;
      return () => {
        view.destroy();
        viewRef.current = null;
      };
      // Deliberately empty deps: the view is built once per mount; external
      // `value` changes (e.g. switching notes) are synced via the effect below
      // rather than rebuilding the whole editor.
      // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    useEffect(() => {
      const view = viewRef.current;
      if (!view) return;
      const current = view.state.doc.toString();
      if (current !== value) {
        view.dispatch({ changes: { from: 0, to: current.length, insert: value } });
      }
    }, [value]);

    return <div ref={containerRef} style={{ height: "100%", minHeight: 0 }} />;
  },
);
