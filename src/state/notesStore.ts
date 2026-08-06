import { useCallback, useEffect, useState } from "react";
import { createNode, deleteNode as apiDeleteNode, listNodes } from "../api/nodes";
import type { ActorKind, NodeSummaryDto, NodeType } from "../types/node";

const ACTOR_FILTER_KEY = "noteapp.actorFilter";

// Defaults to "mine only" the very first time the app ever loads (nothing
// stored yet), then remembers whatever the user last picked. "" is a valid
// stored choice meaning "everyone" — only a missing key falls back to "user".
function loadStoredActor(): ActorKind | undefined {
  const stored = localStorage.getItem(ACTOR_FILTER_KEY);
  if (stored === null) return "user";
  return (stored || undefined) as ActorKind | undefined;
}

export function useNotesStore() {
  const [nodeType, setNodeType] = useState<NodeType | undefined>(undefined);
  const [actor, setActorState] = useState<ActorKind | undefined>(loadStoredActor);
  const setActor = useCallback((next: ActorKind | undefined) => {
    localStorage.setItem(ACTOR_FILTER_KEY, next ?? "");
    setActorState(next);
  }, []);
  const [items, setItems] = useState<NodeSummaryDto[]>([]);
  const [listLoading, setListLoading] = useState(true);

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

  // Creates a note and returns its id — callers decide which pane/tab (if
  // any) to open it into; the store itself no longer tracks "the" selection
  // now that multiple notes can be open at once across tabs/split panes.
  const createNote = useCallback(
    async (title: string, type: NodeType = "page", folderPath?: string) => {
      const result = await createNode({
        title,
        node_type: type,
        content: "",
        properties: folderPath ? [{ key: "folder", value_type: "text", value_text: folderPath }] : [],
      });
      await refreshList();
      return result.id;
    },
    [refreshList],
  );

  const deleteNode = useCallback(
    async (id: string) => {
      await apiDeleteNode(id);
      await refreshList();
    },
    [refreshList],
  );

  const findByTitle = useCallback(
    (title: string) => {
      const normalized = title.trim().toLowerCase();
      return items.find((n) => n.title.trim().toLowerCase() === normalized) ?? null;
    },
    [items],
  );

  return {
    nodeType,
    setNodeType,
    actor,
    setActor,
    items,
    listLoading,
    refreshList,
    createNote,
    deleteNode,
    findByTitle,
  };
}

export type NotesStore = ReturnType<typeof useNotesStore>;
