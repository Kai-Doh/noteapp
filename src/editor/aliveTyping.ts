import { StateEffect, StateField, type Extension } from "@codemirror/state";
import { Decoration, EditorView, ViewPlugin, type DecorationSet, type ViewUpdate } from "@codemirror/view";

// Marks freshly-inserted text so it can fade/pop in via CSS (.cm-just-typed)
// instead of just appearing. Cleared a beat after typing stops so the
// decoration set doesn't grow unbounded during a long editing session.
const addTyped = StateEffect.define<{ from: number; to: number }[]>();
const clearTyped = StateEffect.define<null>();

const typedMark = Decoration.mark({ class: "cm-just-typed" });

const typedField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(marks, tr) {
    marks = marks.map(tr.changes);
    for (const effect of tr.effects) {
      if (effect.is(addTyped)) {
        marks = marks.update({ add: effect.value.map((r) => typedMark.range(r.from, r.to)), sort: true });
      } else if (effect.is(clearTyped)) {
        marks = Decoration.none;
      }
    }
    return marks;
  },
  provide: (f) => EditorView.decorations.from(f),
});

// The cursor "pulse" is purely a DOM class toggle on every doc change (not
// tied to a text position, so no need to route it through EditorState).
const typingLifePlugin = ViewPlugin.fromClass(
  class {
    clearTimer: number | null = null;

    update(update: ViewUpdate) {
      if (!update.docChanged) return;

      const ranges: { from: number; to: number }[] = [];
      update.changes.iterChanges((_fromA, _toA, fromB, toB) => {
        if (toB > fromB) ranges.push({ from: fromB, to: toB });
      });

      const view = update.view;
      view.dom.classList.add("cm-typing-pulse");

      if (this.clearTimer) window.clearTimeout(this.clearTimer);
      this.clearTimer = window.setTimeout(() => {
        view.dom.classList.remove("cm-typing-pulse");
        view.dispatch({ effects: clearTyped.of(null) });
      }, 260);

      if (ranges.length > 0) {
        // Deferred to a microtask: dispatching straight from inside a
        // ViewPlugin's update() re-enters CodeMirror's update cycle, which
        // it doesn't allow — this runs right after the current update
        // finishes instead, before any further input can land.
        Promise.resolve().then(() => {
          view.dispatch({ effects: addTyped.of(ranges) });
        });
      }
    }

    destroy() {
      if (this.clearTimer) window.clearTimeout(this.clearTimer);
    }
  },
);

export function aliveTyping(): Extension {
  return [typedField, typingLifePlugin];
}
