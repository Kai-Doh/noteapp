import { useCallback, useEffect, useRef, useState } from "react";
import "./App.css";
import { FolderTree } from "./components/FolderTree";
import { Dashboard } from "./components/Dashboard";
import { SearchPalette } from "./components/SearchPalette";
import { ReviewQueuePanel } from "./components/ReviewQueuePanel";
import { AiActivityFeed } from "./components/AiActivityFeed";
import { GraphView } from "./components/GraphView";
import { ConnectionSettings } from "./components/ConnectionSettings";
import { SettingsDialog } from "./components/SettingsDialog";
import { UpdateBanner } from "./components/UpdateBanner";
import { NoteEditorPane } from "./components/NoteEditorPane";
import { PaneWorkspace } from "./components/PaneWorkspace";
import { useNotesStore } from "./state/notesStore";
import { patchNode } from "./api/nodes";
import { apiFetch } from "./api/client";
import { getServerConfig } from "./api/connection";
import {
  closeTab,
  closeTabEverywhere,
  collectLeaves,
  findLeaf,
  loadPaneTree,
  moveTab,
  openInPane,
  savePaneTree,
  setActiveTab,
  splitWithTab,
  updateRatio,
  type LeafPane,
  type PaneNode,
  type SplitEdge,
} from "./state/paneTree";

type ViewMode = "notes" | "memory" | "graph";
type ConnectionState = "checking" | "unconfigured" | "unreachable" | "connected";

const VIEW_MODE_INDEX: Record<ViewMode, number> = { notes: 0, memory: 1, graph: 2 };

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

  const [viewMode, setViewMode] = useState<ViewMode>("notes");
  const [searchOpen, setSearchOpen] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  // Persisted so the layout choice survives a restart, not just this session.
  const [leftCollapsed, setLeftCollapsed] = useState(
    () => localStorage.getItem("noteapp.leftCollapsed") === "1",
  );
  const [rightCollapsed, setRightCollapsed] = useState(
    () => localStorage.getItem("noteapp.rightCollapsed") === "1",
  );

  const [paneTree, setPaneTree] = useState<PaneNode>(() => loadPaneTree());
  const [activePaneId, setActivePaneId] = useState<string>(() => collectLeaves(paneTree)[0].id);
  const activePaneIdRef = useRef(activePaneId);
  useEffect(() => {
    activePaneIdRef.current = activePaneId;
  }, [activePaneId]);

  useEffect(() => {
    localStorage.setItem("noteapp.leftCollapsed", leftCollapsed ? "1" : "0");
  }, [leftCollapsed]);
  useEffect(() => {
    localStorage.setItem("noteapp.rightCollapsed", rightCollapsed ? "1" : "0");
  }, [rightCollapsed]);
  useEffect(() => {
    savePaneTree(paneTree);
  }, [paneTree]);
  // A pane can disappear out from under the active selection (its last tab
  // closed and the split it lived in collapsed) — always fall back to some
  // still-existing leaf so "+New note"/sidebar clicks have somewhere to go.
  useEffect(() => {
    if (!findLeaf(paneTree, activePaneId)) {
      setActivePaneId(collectLeaves(paneTree)[0].id);
    }
  }, [paneTree, activePaneId]);

  const openNoteInPane = useCallback((paneId: string, nodeId: string) => {
    setPaneTree((t) => openInPane(t, paneId, nodeId));
    setActivePaneId(paneId);
  }, []);

  const focusPane = useCallback((paneId: string) => setActivePaneId(paneId), []);

  const activateTab = useCallback((paneId: string, nodeId: string) => {
    setPaneTree((t) => setActiveTab(t, paneId, nodeId));
    setActivePaneId(paneId);
  }, []);

  const closeTabHandler = useCallback((paneId: string, nodeId: string) => {
    setPaneTree((t) => closeTab(t, paneId, nodeId));
  }, []);

  const moveTabHandler = useCallback((nodeId: string, sourcePaneId: string, targetPaneId: string) => {
    setPaneTree((t) => moveTab(t, sourcePaneId, targetPaneId, nodeId));
    setActivePaneId(targetPaneId);
  }, []);

  const splitHandler = useCallback(
    (targetPaneId: string, edge: SplitEdge, nodeId: string, sourcePaneId: string) => {
      setPaneTree((t) => splitWithTab(t, targetPaneId, edge, nodeId, sourcePaneId));
      setActivePaneId(targetPaneId);
    },
    [],
  );

  const resizeHandler = useCallback((splitId: string, ratio: number) => {
    setPaneTree((t) => updateRatio(t, splitId, ratio));
  }, []);

  const handleNavigateToTitle = useCallback(
    async (paneId: string, rawTitle: string) => {
      const match = store.findByTitle(rawTitle);
      if (match) {
        openNoteInPane(paneId, match.id);
        return;
      }
      if (window.confirm(`No note titled "${rawTitle}" yet. Create it?`)) {
        const id = await store.createNote(rawTitle);
        openNoteInPane(paneId, id);
      }
    },
    [store, openNoteInPane],
  );

  const handleCreateNote = useCallback(
    async (folderPath?: string) => {
      const id = await store.createNote("Untitled", "page", folderPath);
      setViewMode("notes");
      openNoteInPane(activePaneIdRef.current, id);
    },
    [store, openNoteInPane],
  );

  const handleDeleteNote = useCallback(
    async (id: string) => {
      await store.deleteNode(id);
      setPaneTree((t) => closeTabEverywhere(t, id));
    },
    [store],
  );

  const handleMoveNote = useCallback(
    async (id: string, folderPath: string) => {
      await patchNode(id, {
        properties: [{ key: "folder", value_type: "text", value_text: folderPath || null }],
      });
      await store.refreshList();
    },
    [store],
  );

  // Ctrl/Cmd+K: open the search palette. Save-flushing is handled per open
  // tab now (see NoteEditorPane), since more than one can be open at once.
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      const mod = e.ctrlKey || e.metaKey;
      if (mod && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setSearchOpen(true);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

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

  const activeLeaf = findLeaf(paneTree, activePaneId);

  function renderLeafContent(pane: LeafPane) {
    if (!pane.activeId) {
      return (
        <Dashboard
          items={store.items}
          onSelect={(id) => openNoteInPane(pane.id, id)}
          onQuickCapture={async (t) => {
            const id = await store.createNote(t);
            openNoteInPane(pane.id, id);
          }}
        />
      );
    }
    return (
      <NoteEditorPane
        key={pane.activeId}
        nodeId={pane.activeId}
        onNavigateToTitle={(title) => handleNavigateToTitle(pane.id, title)}
        onOpenById={(id) => openNoteInPane(pane.id, id)}
        onNoteSaved={() => store.refreshList()}
        rightCollapsed={rightCollapsed}
        onToggleRight={() => setRightCollapsed((c) => !c)}
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
              selectedId={activeLeaf?.activeId ?? null}
              onSelect={(id) => openNoteInPane(activePaneIdRef.current, id)}
              loading={store.listLoading}
              onCreateNote={handleCreateNote}
              onDeleteNote={handleDeleteNote}
              onMoveNote={handleMoveNote}
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
          <GraphView
            onSelect={(id) => {
              setViewMode("notes");
              openNoteInPane(activePaneIdRef.current, id);
            }}
          />
        </main>
      )}

      {viewMode === "notes" && (
        <main className="main-pane pane-workspace">
          <PaneWorkspace
            tree={paneTree}
            activePaneId={activePaneId}
            items={store.items}
            renderLeafContent={renderLeafContent}
            onFocusPane={focusPane}
            onActivateTab={activateTab}
            onCloseTab={closeTabHandler}
            onMoveTab={moveTabHandler}
            onSplitWithTab={splitHandler}
            onResize={resizeHandler}
          />
        </main>
      )}

      <SearchPalette
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        onSelect={(id) => openNoteInPane(activePaneIdRef.current, id)}
      />
      </div>
    </>
  );
}

export default App;
