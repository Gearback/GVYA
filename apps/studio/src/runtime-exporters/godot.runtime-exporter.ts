import { buildGodotRuntimeBundle } from "../godot-export.js";
import type { StudioRuntimeExporter } from "../runtime-exporters.js";

export const runtimeExporter: StudioRuntimeExporter = {
  id: "godot",
  label: "Godot runtime",
  order: 20,
  build: buildGodotRuntimeBundle,
};
