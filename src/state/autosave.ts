import { useCallback, useEffect, useRef } from "react";

// Idle-save threshold per the plan: draft state is separate from the
// committed revision, and a durable save fires on manual save, focus loss
// after a meaningful change, N minutes idle, or a property/link mutation —
// never per keystroke.
const IDLE_SAVE_MS = 5 * 60 * 1000;

interface UseAutosaveOptions {
  nodeId: string | null;
  title: string;
  content: string;
  onSave: (patch: { title?: string; content?: string }) => Promise<void>;
}

export function useAutosave({ nodeId, title, content, onSave }: UseAutosaveOptions) {
  const savedRef = useRef<{ nodeId: string | null; title: string; content: string }>({
    nodeId,
    title,
    content,
  });
  const idleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const savingRef = useRef(false);

  const isDirty = useCallback(() => {
    const saved = savedRef.current;
    return saved.nodeId === nodeId && (saved.title !== title || saved.content !== content);
  }, [nodeId, title, content]);

  const flush = useCallback(async () => {
    if (idleTimer.current) {
      clearTimeout(idleTimer.current);
      idleTimer.current = null;
    }
    if (!nodeId || savingRef.current || !isDirty()) return;
    savingRef.current = true;
    try {
      await onSave({ title, content });
      savedRef.current = { nodeId, title, content };
    } finally {
      savingRef.current = false;
    }
  }, [nodeId, title, content, isDirty, onSave]);

  // Switching notes: reset the saved baseline to the freshly loaded note.
  // Callers are responsible for flushing the *previous* note before
  // switching selection away from it.
  useEffect(() => {
    savedRef.current = { nodeId, title, content };
    if (idleTimer.current) {
      clearTimeout(idleTimer.current);
      idleTimer.current = null;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [nodeId]);

  useEffect(() => {
    if (!isDirty()) return;
    if (idleTimer.current) clearTimeout(idleTimer.current);
    idleTimer.current = setTimeout(() => {
      flush();
    }, IDLE_SAVE_MS);
    return () => {
      if (idleTimer.current) clearTimeout(idleTimer.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [title, content]);

  return { flush, isDirty };
}
