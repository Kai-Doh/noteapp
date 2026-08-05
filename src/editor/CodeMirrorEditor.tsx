import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { wikilinkAutocomplete, wikilinkClickNavigation, wikilinkDecorations } from "./wikilinkExtension";
import { markdownDecorations } from "./markdownDecorations";

interface CodeMirrorEditorProps {
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  onNavigateToTitle: (title: string) => void;
}

export function CodeMirrorEditor({ value, onChange, onBlur, onNavigateToTitle }: CodeMirrorEditorProps) {
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

  useEffect(() => {
    if (!containerRef.current) return;

    const state = EditorState.create({
      doc: value,
      extensions: [
        lineNumbers(),
        history(),
        highlightActiveLine(),
        keymap.of([...defaultKeymap, ...historyKeymap]),
        markdown(),
        markdownDecorations(),
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
          ".cm-md-header": { fontFamily: "ui-sans-serif, system-ui, sans-serif", fontWeight: "700" },
          ".cm-md-h1": { fontSize: "1.6em" },
          ".cm-md-h2": { fontSize: "1.4em" },
          ".cm-md-h3": { fontSize: "1.25em" },
          ".cm-md-h4": { fontSize: "1.12em" },
          ".cm-md-h5": { fontSize: "1em" },
          ".cm-md-h6": { fontSize: "0.92em", opacity: "0.85" },
          ".cm-md-strong": { fontWeight: "700" },
          ".cm-md-em": { fontStyle: "italic" },
          ".cm-md-code": {
            fontFamily: "ui-monospace, SFMono-Regular, monospace",
            background: "color-mix(in srgb, currentColor 10%, transparent)",
            borderRadius: "3px",
            padding: "0 3px",
          },
          ".cm-md-quote": { color: "var(--muted, #767676)", fontStyle: "italic" },
          ".cm-md-mark": { opacity: "0.4" },
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
}
