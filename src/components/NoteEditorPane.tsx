import { useCallback, useEffect, useRef, useState } from "react";
import { CodeMirrorEditor, type CodeMirrorEditorHandle, type FormatKind } from "../editor/CodeMirrorEditor";
import { PropertiesPanel } from "./PropertiesPanel";
import { BacklinksPanel } from "./BacklinksPanel";
import { useAutosave } from "../state/autosave";
import { getNode, patchNode } from "../api/nodes";
import type { NodeDto, PropertyInput } from "../types/node";

const TOOLBAR_BUTTONS: { kind: FormatKind; title: string; icon: React.ReactNode }[] = [
  { kind: "bold", title: "Bold", icon: <span style={{ fontWeight: 700 }}>B</span> },
  { kind: "italic", title: "Italic", icon: <span style={{ fontStyle: "italic" }}>I</span> },
  {
    kind: "list",
    title: "List",
    icon: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round">
        <circle cx="4.5" cy="6" r="1.1" fill="currentColor" stroke="none" />
        <path d="M9 6h11" />
        <circle cx="4.5" cy="12" r="1.1" fill="currentColor" stroke="none" />
        <path d="M9 12h11" />
        <circle cx="4.5" cy="18" r="1.1" fill="currentColor" stroke="none" />
        <path d="M9 18h11" />
      </svg>
    ),
  },
  {
    kind: "quote",
    title: "Quote",
    icon: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor">
        <path d="M7 8c-2 0-3 1.5-3 3.5S5 15 7 15v3c-3 0-5.5-2.5-5.5-6.5S4 5 7 5v3zM17 8c-2 0-3 1.5-3 3.5S15 15 17 15v3c-3 0-5.5-2.5-5.5-6.5S14 5 17 5v3z" />
      </svg>
    ),
  },
  {
    kind: "link",
    title: "Link",
    icon: (
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round">
        <path d="M9.5 14.5l5-5M8 11.2 5.6 13.6a3 3 0 0 0 4.2 4.2L12 15.6M16 12.8l2.4-2.4a3 3 0 0 0-4.2-4.2L12 8.4" />
      </svg>
    ),
  },
];

interface NoteEditorPaneProps {
  nodeId: string;
  onNavigateToTitle: (title: string) => void;
  onOpenById: (id: string) => void;
  onNoteSaved: () => void;
  rightCollapsed: boolean;
  onToggleRight: () => void;
  previewMode: boolean;
  onTogglePreview: () => void;
}

// One instance per open tab's active render — owns the draft/autosave state
// for whichever note it's currently pointed at, mirroring how a single
// global selection used to work before tabs existed, just scoped per pane.
export function NoteEditorPane({
  nodeId,
  onNavigateToTitle,
  onOpenById,
  onNoteSaved,
  rightCollapsed,
  onToggleRight,
  previewMode,
  onTogglePreview,
}: NoteEditorPaneProps) {
  const [node, setNode] = useState<NodeDto | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [folder, setFolder] = useState("");
  const [savedAt, setSavedAt] = useState<Date | null>(null);
  const editorRef = useRef<CodeMirrorEditorHandle>(null);

  const load = useCallback(async () => {
    setLoadError(null);
    try {
      const fresh = await getNode(nodeId);
      setNode(fresh);
      setTitle(fresh.title);
      setContent(fresh.content);
      setFolder(fresh.properties.find((p) => p.key === "folder")?.value_text ?? "");
    } catch (err) {
      setLoadError(err instanceof Error ? err.message : String(err));
    }
  }, [nodeId]);

  useEffect(() => {
    setNode(null);
    load();
  }, [load]);

  const handleSave = useCallback(
    async (patch: { title?: string; content?: string }) => {
      await patchNode(nodeId, patch);
      setSavedAt(new Date());
      const fresh = await getNode(nodeId);
      setNode(fresh);
      onNoteSaved();
    },
    [nodeId, onNoteSaved],
  );

  const autosave = useAutosave({ nodeId, title, content, onSave: handleSave });

  // Best-effort: flush any unsaved draft if this pane instance goes away
  // (tab switched, split closed, or the whole workspace unmounted).
  useEffect(() => () => { autosave.flush(); }, [autosave]);

  const handlePropertiesChange = useCallback(
    async (properties: PropertyInput[]) => {
      await patchNode(nodeId, { properties });
      setSavedAt(new Date());
      const fresh = await getNode(nodeId);
      setNode(fresh);
      onNoteSaved();
    },
    [nodeId, onNoteSaved],
  );

  const handleFolderBlur = useCallback(async () => {
    if (!node) return;
    const trimmed = folder.trim();
    const existing = node.properties.find((p) => p.key === "folder")?.value_text ?? "";
    if (trimmed === existing) return;
    await handlePropertiesChange([{ key: "folder", value_type: "text", value_text: trimmed || null }]);
  }, [folder, node, handlePropertiesChange]);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        autosave.flush();
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [autosave]);

  if (loadError) {
    return <div className="panel-empty">Couldn't load this note: {loadError}</div>;
  }
  if (!node) {
    return <div className="panel-empty">Loading…</div>;
  }

  return (
    <div className="note-editor-pane">
      <div className="note-view">
        <div className="note-header">
          <input
            className="note-title-input"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            onBlur={() => autosave.flush()}
          />
          <input
            className="note-folder-input"
            value={folder}
            placeholder="Folder (e.g. Fitness/Nutrition)"
            onChange={(e) => setFolder(e.target.value)}
            onBlur={handleFolderBlur}
          />
          <span className="save-status">{savedAt ? `Saved ${savedAt.toLocaleTimeString()}` : " "}</span>
          <button
            className="collapse-trigger"
            onClick={onToggleRight}
            title={rightCollapsed ? "Show properties/backlinks" : "Hide properties/backlinks"}
          >
            {rightCollapsed ? "«" : "»"}
          </button>
        </div>
        <div className="editor-toolbar">
          {TOOLBAR_BUTTONS.map((btn) => (
            <button
              key={btn.kind}
              type="button"
              className="editor-toolbar-btn"
              title={btn.title}
              onClick={() => editorRef.current?.applyFormat(btn.kind)}
            >
              {btn.icon}
            </button>
          ))}
          <span className="editor-toolbar-divider" />
          <button
            type="button"
            className={`editor-toolbar-btn${previewMode ? " active" : ""}`}
            title={previewMode ? "Show raw markdown characters" : "Hide markdown characters (clean preview)"}
            onClick={onTogglePreview}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8} strokeLinecap="round" strokeLinejoin="round">
              <path d="M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7-10-7-10-7Z" />
              <circle cx="12" cy="12" r="3" />
            </svg>
          </button>
        </div>
        <div className="note-editor">
          <CodeMirrorEditor
            ref={editorRef}
            value={content}
            onChange={setContent}
            onBlur={() => autosave.flush()}
            onNavigateToTitle={onNavigateToTitle}
            previewMode={previewMode}
          />
        </div>
      </div>

      {!rightCollapsed && (
        <aside className="right-panel">
          <PropertiesPanel properties={node.properties} onChange={handlePropertiesChange} />

          <div className="outgoing-links">
            <h3>Outgoing links</h3>
            {node.links.length === 0 && <div className="panel-empty">No links in this note.</div>}
            <ul>
              {node.links.map((l) => (
                <li key={l.id}>
                  <button
                    className={`outgoing-link outgoing-link-${l.status}`}
                    disabled={!l.target_node_id}
                    onClick={() => l.target_node_id && onOpenById(l.target_node_id)}
                    title={l.status}
                  >
                    {l.display_text ?? l.target_raw}
                  </button>
                </li>
              ))}
            </ul>
          </div>

          <BacklinksPanel nodeId={nodeId} onSelect={onOpenById} />
        </aside>
      )}
    </div>
  );
}
