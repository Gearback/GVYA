import type { StudioAssetFile, StudioBrainWorkspace } from "./types.js";

export interface RuntimeExportBundle {
  filename: string;
  mediaType: string;
  bytes: Uint8Array;
}

export interface StudioRuntimeExporter {
  id: string;
  label: string;
  order: number;
  build: (workspace: StudioBrainWorkspace, assetFiles: readonly StudioAssetFile[]) => Promise<RuntimeExportBundle>;
}

interface RuntimeExporterModule {
  runtimeExporter: StudioRuntimeExporter;
}

// Drop-in registry: one `*.runtime-exporter.ts` module contributes one Build-tab export action.
const modules = import.meta.glob<RuntimeExporterModule>("./runtime-exporters/*.runtime-exporter.ts", { eager: true });
const exporters = Object.entries(modules).map(([path, module]) => {
  const exporter = module.runtimeExporter;
  if (!exporter || !/^[a-z0-9-]+$/u.test(exporter.id) || !exporter.label.trim() || !Number.isFinite(exporter.order) || typeof exporter.build !== "function") {
    throw new Error(`Invalid Studio runtime exporter: ${path}`);
  }
  return exporter;
});

const exporterIds = new Set<string>();
for (const exporter of exporters) {
  if (exporterIds.has(exporter.id)) throw new Error(`Duplicate Studio runtime exporter ID: ${exporter.id}`);
  exporterIds.add(exporter.id);
}

export const RUNTIME_EXPORTERS: readonly StudioRuntimeExporter[] = Object.freeze(
  exporters.sort((left, right) => left.order - right.order || left.label.localeCompare(right.label)),
);
