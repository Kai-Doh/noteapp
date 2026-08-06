import { useCallback, useEffect, useRef, useState } from "react";
import "./App.css";
import { CodeMirrorEditor, type CodeMirrorEditorHandle, type FormatKind } from "./editor/CodeMirrorEditor";
import { FolderTree } from "./components/FolderTree";
import { Dashboard } from "./components/Dashboard";
import { BacklinksPanel } from "./components/BacklinksPanel";
import { PropertiesPanel } from "./components/PropertiesPanel";
import { SearchPalette } from "./components/SearchPalette";
import { ReviewQueuePanel } from "./components/ReviewQueuePanel";
import { AiActivityFeed } from "./components/AiActivityFeed";
import { GraphView } from "./components/GraphView";
import { ConnectionSettings } from "./components/ConnectionSettings";
import { SettingsDialog } from "./components/SettingsDialog";
import { UpdateBanner } from "./components/UpdateBanner";
import { useNotesStore } from "./state/notesStore";
import { useAutosave } from "./state/autosave";
import { patchNode } from "./api/nodes";
import { apiFetch } from "./api/client";
import { getServerConfig } from "./api/connection";
import type { PropertyInput } from "./types/node";

type ViewMode = "notes" | "memory" | "graph";
type ConnectionState = "checking" | "unconfigured" | "unreachable" | "connected";

const VIEW_MODE_INDEX: Record<ViewMode, number> = { notes: 0, memory: 1, graph: 2 };

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

function App() {
  const [connectionState, setConnectionState] = useState<ConnectionState>("checking");
  const [checkError, setCheckError] = useState<string | null>(null);

  const checkConnection = useCallback(async () => {
    setConnectionState("checking");
    let cfg;
    try {
      cfg = await getServerConfig();
    } catch (err) {
      // Tauri IPC itself failing (not just an unreachable server) is
      // unexpected, but still shouldn't leave the user stuck on a spinner
      // forever with no way out — fall back to the settings screen.
      setCheckError(err instanceof Error ? err.message : String(err));
      setConnectionState("unconfigured");
      return;
    }
    if (!cfg.url || !cfg.has_token) {
      setConnectionState("unconfigured");
      return;
    }
    try {
      await apiFetch("/nodes?limit=1");
      setConnectionState("connected");
    } catch (err) {
      setCheckError(err instanceof Error ? err.message : String(err));
      setConnectionState("unreachable");
    }
  }, []);

  useEffect(() => {
    checkConnection();
  }, [checkConnection]);

  if (connectionState === "checking") {
    return <div className="connection-checking">Connecting…</div>;
  }

  if (connectionState === "unconfigured" || connectionState === "unreachable") {
    return (
      <div className="dialog-backdrop">
        <div className="dialog" style={{ width: "min(420px, 100%)" }}>
          <ConnectionSettings
            onConnected={checkConnection}
            banner={connectionState === "unreachable" ? `Couldn't reach the server: ${checkError}` : undefined}
          />
        </div>
      </div>
    );
  }

  return <VaultApp />;
}

function VaultApp() {
  const store = useNotesStore();
  const { selectedNode, selectedId } = store;

  const [viewMode, setViewMode] = useState<ViewMode>("notes");
  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [folder, setFolder] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [savedAt, setSavedAt] = useState<Date | null>(null);
  // Persisted so the layout choice survives a restart, not just this session.
  const [leftCollapsed, setLeftCollapsed] = useState(
    () => localStorage.getItem("noteapp.leftCollapsed") === "1",
  );
  const [rightCollapsed, setRightCollapsed] = useState(
    () => localStorage.getItem("noteapp.rightCollapsed") === "1",
  );

  useEffect(() => {
    localStorage.setItem("noteapp.leftCollapsed", leftCollapsed ? "1" : "0");
  }, [leftCollapsed]);
  useEffect(() => {
    localStorage.setItem("noteapp.rightCollapsed", rightCollapsed ? "1" : "0");
  }, [rightCollapsed]);

  // Sync local draft state whenever a (possibly different) note finishes loading.
  useEffect(() => {
    if (selectedNode) {
      setTitle(selectedNode.title);
      setContent(selectedNode.content);
      setFolder(selectedNode.properties.find((p) => p.key === "folder")?.value_text ?? "");
    }
  }, [selectedNode?.id]); // eslint-disable-line react-hooks/exhaustive-deps

  const handleSave = useCallback(
    async (patch: { title?: string; content?: string }) => {
      if (!selectedId) return;
      await patchNode(selectedId, patch);
      setSavedAt(new Date());
      await Promise.all([store.refreshSelected(), store.refreshList()]);
    },
    [selectedId, store],
  );

  const autosave = useAutosave({ nodeId: selectedId, title, content, onSave: handleSave });

  const handlePropertiesChange = useCallback(
    async (properties: PropertyInput[]) => {
      if (!selectedId) return;
      // Property changes save immediately (not on the idle timer) per the
      // plan: property/link mutations are one of the explicit save triggers.
      await patchNode(selectedId, { properties });
      setSavedAt(new Date());
      await store.refreshSelected();
    },
    [selectedId, store],
  );

  const handleFolderBlur = useCallback(async () => {
    if (!selectedNode) return;
    const trimmed = folder.trim();
    const existing = selectedNode.properties.find((p) => p.key === "folder")?.value_text ?? "";
    if (trimmed === existing) return;
    await handlePropertiesChange([{ key: "folder", value_type: "text", value_text: trimmed || null }]);
    await store.refreshList();
  }, [folder, selectedNode, handlePropertiesChange, store]);

  const editorRef = useRef<CodeMirrorEditorHandle>(null);

  const selectAndFlush = useCallback(
    async (id: string) => {
      await autosave.flush();
      await store.selectNode(id);
    },
    [autosave, store],
  );

  const handleNavigateToTitle = useCallback(
    async (rawTitle: string) => {
      const existingId = await store.selectByTitle(rawTitle);
      if (existingId) {
        await selectAndFlush(existingId);
        return;
      }
      if (window.confirm(`No note titled "${rawTitle}" yet. Create it?`)) {
        await autosave.flush();
        await store.createAndSelect(rawTitle);
      }
    },
    [store, autosave, selectAndFlush],
  );

  const handleCreateNote = useCallback(
    async (folderPath?: string) => {
      const id = await store.createAndSelect("Untitled", "page", folderPath);
      setViewMode("notes");
      await store.selectNode(id);
    },
    [store],
  );

  // Ctrl/Cmd+S: manual save. Ctrl/Cmd+K: open the search palette.
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "s") {
        e.preventDefault();
        autosave.flush();
      } else if (mod && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen(true);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [autosave]);

  if (showSettings) {
    return (
      <SettingsDialog
        onConnected={() => {
          // Reconfiguring mid-session touches every cached client/server
          // assumption (different vault entirely, potentially) — a full
          // reload is the simplest way to guarantee nothing stale lingers.
          window.location.reload();
        }}
        onCancel={() => setShowSettings(false)}
      />
    );
  }

  return (
    <>
      <UpdateBanner />
      <div className="app-shell">
      <aside className={`sidebar${leftCollapsed ? " sidebar-collapsed" : ""}`}>
        <div className="sidebar-content">
          <div className="sidebar-header">
            <span className="brand-mark">N</span>
            <span className="app-title">noteapp</span>
          </div>
          <button className="sidebar-new-note-btn" onClick={() => handleCreateNote()}>
            + New note
          </button>
          <button className="search-trigger" onClick={() => setSearchOpen(true)}>
            Search <span className="kbd">Ctrl+K</span>
          </button>
          <div className="view-mode-toggle">
            <div
              className="view-mode-indicator"
              style={{ transform: `translateX(${VIEW_MODE_INDEX[viewMode] * 100}%)` }}
            />
            <button className={viewMode === "notes" ? "active" : ""} onClick={() => setViewMode("notes")}>
              Notes
            </button>
            <button className={viewMode === "memory" ? "active" : ""} onClick={() => setViewMode("memory")}>
              Memory
            </button>
            <button className={viewMode === "graph" ? "active" : ""} onClick={() => setViewMode("graph")}>
              Graph
            </button>
          </div>
          {viewMode === "notes" && (
            <FolderTree
              items={store.items}
              nodeType={store.nodeType}
              onNodeTypeChange={store.setNodeType}
              actor={store.actor}
              onActorChange={store.setActor}
              selectedId={selectedId}
              onSelect={selectAndFlush}
              loading={store.listLoading}
              onCreateNote={handleCreateNote}
              onDeleteNote={store.deleteNode}
            />
          )}
          <button className="reconfigure-trigger" onClick={() => setShowSettings(true)}>
            ⚙ Settings
          </button>
        </div>
        <button
          className="sidebar-toggle-tab"
          onClick={() => setLeftCollapsed((c) => !c)}
          title={leftCollapsed ? "Show sidebar" : "Hide sidebar"}
        >
          {leftCollapsed ? "»" : "«"}
        </button>
      </aside>

      {viewMode === "memory" && (
        <main className="main-pane memory-view">
          <ReviewQueuePanel />
          <AiActivityFeed />
        </main>
      )}

      {viewMode === "graph" && (
        <main className="main-pane graph-pane">
          <GraphView onSelect={(id) => { setViewMode("notes"); selectAndFlush(id); }} />
        </main>
      )}

      {viewMode === "notes" && (
        <>
          <main className="main-pane">
            {!selectedId && (
              <Dashboard
                items={store.items}
                onSelect={selectAndFlush}
                onQuickCapture={async (t) => {
                  await store.createAndSelect(t);
                }}
              />
            )}

            {selectedId && selectedNode && (
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
                  <span className="save-status">
                    {savedAt ? `Saved ${savedAt.toLocaleTimeString()}` : " "}
                  </span>
                  <button
                    className="collapse-trigger"
                    onClick={() => setRightCollapsed((c) => !c)}
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
                </div>
                <div className="note-editor">
                  <CodeMirrorEditor
                    ref={editorRef}
                    value={content}
                    onChange={setContent}
                    onBlur={() => autosave.flush()}
                    onNavigateToTitle={handleNavigateToTitle}
                  />
                </div>
              </div>
            )}

            {selectedId && !selectedNode && <div className="panel-empty">Loading…</div>}
          </main>

          {selectedId && selectedNode && !rightCollapsed && (
            <aside className="right-panel">
              <PropertiesPanel properties={selectedNode.properties} onChange={handlePropertiesChange} />

              <div className="outgoing-links">
                <h3>Outgoing links</h3>
                {selectedNode.links.length === 0 && <div className="panel-empty">No links in this note.</div>}
                <ul>
                  {selectedNode.links.map((l) => (
                    <li key={l.id}>
                      <button
                        className={`outgoing-link outgoing-link-${l.status}`}
                        disabled={!l.target_node_id}
                        onClick={() => l.target_node_id && selectAndFlush(l.target_node_id)}
                        title={l.status}
                      >
                        {l.display_text ?? l.target_raw}
                      </button>
                    </li>
                  ))}
                </ul>
              </div>

              <BacklinksPanel nodeId={selectedId} onSelect={selectAndFlush} />
            </aside>
          )}
        </>
      )}

      <SearchPalette
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onSelect={(id) => selectAndFlush(id)}
      />
      </div>
    </>
  );
}

export default App;
