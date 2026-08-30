import { buildWebRuntimeBundle } from "../web-export.js";
import type { StudioRuntimeExporter } from "../runtime-exporters.js";

export const runtimeExporter: StudioRuntimeExporter = {
  id: "web",
  label: "Web runtime",
  order: 10,
  build: buildWebRuntimeBundle,
};
