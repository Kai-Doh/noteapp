import { useCallback, useEffect, useState } from "react";
import { createNode, deleteNode as apiDeleteNode, getNode, listNodes } from "../api/nodes";
import type { ActorKind, NodeDto, NodeSummaryDto, NodeType } from "../types/node";

export function useNotesStore() {
  const [nodeType, setNodeType] = useState<NodeType | undefined>(undefined);
  const [actor, setActor] = useState<ActorKind | undefined>(undefined);
  const [items, setItems] = useState<NodeSummaryDto[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<NodeDto | null>(null);
  const [listLoading, setListLoading] = useState(true);
  const [noteLoading, setNoteLoading] = useState(false);

  const refreshList = useCallback(async () => {
    setListLoading(true);
    try {
      const res = await listNodes({ node_type: nodeType, created_by: actor, limit: 200 });
      setItems(res.items);
    } finally {
      setListLoading(false);
    }
  }, [nodeType, actor]);

  useEffect(() => {
    refreshList();
  }, [refreshList]);

  const selectNode = useCallback(async (id: string) => {
    setSelectedId(id);
    setNoteLoading(true);
    try {
      const node = await getNode(id);
      setSelectedNode(node);
    } finally {
      setNoteLoading(false);
    }
  }, []);

  // Re-fetches the currently open node (e.g. after a save, so backlinks/
  // outgoing links reflect what the backend just resolved).
  const refreshSelected = useCallback(async () => {
    if (!selectedId) return;
    const node = await getNode(selectedId);
    setSelectedNode(node);
  }, [selectedId]);

  const createAndSelect = useCallback(
    async (title: string, type: NodeType = "page", folderPath?: string) => {
      const result = await createNode({
        title,
        node_type: type,
        content: "",
        properties: folderPath ? [{ key: "folder", value_type: "text", value_text: folderPath }] : [],
      });
      await refreshList();
      await selectNode(result.id);
      return result.id;
    },
    [refreshList, selectNode],
  );

  const deleteNode = useCallback(
    async (id: string) => {
      await apiDeleteNode(id);
      if (selectedId === id) {
        setSelectedId(null);
        setSelectedNode(null);
      }
      await refreshList();
    },
    [selectedId, refreshList],
  );

  const selectByTitle = useCallback(
    async (title: string) => {
      const normalized = title.trim().toLowerCase();
      const match = items.find((n) => n.title.trim().toLowerCase() === normalized);
      if (match) {
        await selectNode(match.id);
        return match.id;
      }
      // Not found locally (e.g. sidebar filtered to a different node_type) —
      // offer to create it, mirroring how unresolved wikilinks work elsewhere
      // in the app: nothing is created implicitly on a click.
      return null;
    },
    [items, selectNode],
  );

  return {
    nodeType,
    setNodeType,
    actor,
    setActor,
    items,
    listLoading,
    refreshList,
    selectedId,
    selectedNode,
    noteLoading,
    selectNode,
    refreshSelected,
    createAndSelect,
    deleteNode,
    selectByTitle,
  };
}

export type NotesStore = ReturnType<typeof useNotesStore>;
