import { useState } from "react";
import type { PropertyInput, PropertyValueType } from "../types/node";

interface PropertiesPanelProps {
  properties: PropertyInput[];
  onChange: (properties: PropertyInput[]) => void;
}

const VALUE_TYPES: PropertyValueType[] = ["text", "number", "bool", "date", "node_ref"];

function valueOf(p: PropertyInput): string {
  switch (p.value_type) {
    case "text":
    case "node_ref":
      return p.value_text ?? "";
    case "number":
      return p.value_number != null ? String(p.value_number) : "";
    case "bool":
      return p.value_bool ? "true" : "false";
    case "date":
      return p.value_date ?? "";
  }
}

function withValue(p: PropertyInput, raw: string): PropertyInput {
  const base: PropertyInput = { key: p.key, value_type: p.value_type };
  switch (p.value_type) {
    case "text":
      return { ...base, value_text: raw };
    case "node_ref":
      return { ...base, value_node_id: raw };
    case "number":
      return { ...base, value_number: raw === "" ? null : Number(raw) };
    case "bool":
      return { ...base, value_bool: raw === "true" };
    case "date":
      return { ...base, value_date: raw };
  }
}

export function PropertiesPanel({ properties, onChange }: PropertiesPanelProps) {
  const [newKey, setNewKey] = useState("");

  function updateAt(index: number, next: PropertyInput) {
    const copy = properties.slice();
    copy[index] = next;
    onChange(copy);
  }

  function removeAt(index: number) {
    onChange(properties.filter((_, i) => i !== index));
  }

  function addProperty() {
    const key = newKey.trim();
    if (!key || properties.some((p) => p.key === key)) return;
    onChange([...properties, { key, value_type: "text", value_text: "" }]);
    setNewKey("");
  }

  return (
    <div className="properties-panel">
      <h3>Properties</h3>
      <ul className="properties-list">
        {properties.map((p, i) => (
          <li key={p.key} className="property-row">
            <span className="property-key">{p.key}</span>
            <select
              value={p.value_type}
              onChange={(e) => updateAt(i, withValue({ ...p, value_type: e.target.value as PropertyValueType }, ""))}
            >
              {VALUE_TYPES.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
            {p.value_type === "bool" ? (
              <select value={valueOf(p)} onChange={(e) => updateAt(i, withValue(p, e.target.value))}>
                <option value="true">true</option>
                <option value="false">false</option>
              </select>
            ) : (
              <input
                type={p.value_type === "number" ? "number" : p.value_type === "date" ? "date" : "text"}
                value={valueOf(p)}
                onChange={(e) => updateAt(i, withValue(p, e.target.value))}
              />
            )}
            <button className="property-remove" onClick={() => removeAt(i)} aria-label={`Remove ${p.key}`}>
              ×
            </button>
          </li>
        ))}
      </ul>
      <div className="property-add">
        <input
          placeholder="New property key"
          value={newKey}
          onChange={(e) => setNewKey(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && addProperty()}
        />
        <button onClick={addProperty} disabled={!newKey.trim()}>
          Add
        </button>
      </div>
    </div>
  );
}
