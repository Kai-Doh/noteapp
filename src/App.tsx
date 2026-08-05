import { useCallback, useEffect, useState } from "react";
import "./App.css";
import { CodeMirrorEditor } from "./editor/CodeMirrorEditor";
import { FolderTree } from "./components/FolderTree";
import { Dashboard } from "./components/Dashboard";
import { BacklinksPanel } from "./components/BacklinksPanel";
import { PropertiesPanel } from "./components/PropertiesPanel";
import { SearchPalette } from "./components/SearchPalette";
import { ReviewQueuePanel } from "./components/ReviewQueuePanel";
import { AiActivityFeed } from "./components/AiActivityFeed";
import { GraphView } from "./components/GraphView";
import { ConnectionSettings } from "./components/ConnectionSettings";
import { useNotesStore } from "./state/notesStore";
import { useAutosave } from "./state/autosave";
import { patchNode } from "./api/nodes";
import { apiFetch } from "./api/client";
import { getServerConfig } from "./api/connection";
import type { PropertyInput } from "./types/node";

type ViewMode = "notes" | "memory" | "graph";
type ConnectionState = "checking" | "unconfigured" | "unreachable" | "connected";

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
      <>
        {connectionState === "unreachable" && (
          <div className="connection-banner">Couldn't reach the server: {checkError}</div>
        )}
        <ConnectionSettings onConnected={checkConnection} />
      </>
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
      <ConnectionSettings
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
    <div className="app-shell">
      {leftCollapsed ? (
        <button
          className="sidebar-expand-strip"
          onClick={() => setLeftCollapsed(false)}
          title="Show sidebar"
        >
          »
        </button>
      ) : (
        <aside className="sidebar">
          <div className="sidebar-header">
            <span className="app-title">noteapp</span>
            <button className="search-trigger" onClick={() => setSearchOpen(true)}>
              Search (Ctrl+K)
            </button>
            <button
              className="collapse-trigger"
              onClick={() => setLeftCollapsed(true)}
              title="Hide sidebar"
            >
              «
            </button>
          </div>
          <div className="view-mode-toggle">
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
              onCreateNote={async (folderPath) => {
                const id = await store.createAndSelect("Untitled", "page", folderPath);
                setViewMode("notes");
                await store.selectNode(id);
              }}
            />
          )}
          <button className="reconfigure-trigger" onClick={() => setShowSettings(true)}>
            ⚙ Server settings
          </button>
        </aside>
      )}

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
                <div className="note-editor">
                  <CodeMirrorEditor
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
  );
}

export default App;
