import { encodeContent } from "./studio-content.js";
import { selectedBot, selectedProject } from "./studio-model.js";
import type { StudioAssetFile, StudioPackage, StudioWorkspace } from "./types.js";
import { createZip, type ZipEntry } from "./zip.js";

export interface FolderZipArchive {
  filename: string;
  mediaType: "application/zip";
  bytes: Uint8Array;
}

export async function buildSelectedPackageFolderZip(workspace: StudioWorkspace, assetFiles: readonly StudioAssetFile[], date = new Date()): Promise<FolderZipArchive> {
  const selected = selectedPackageFolder(workspace);
  return buildFolderZip(workspace, assetFiles, selected.prefix, selected.pkg.manifest.id, selected.pkg.manifest.id, date);
}

export async function buildSelectedBotFolderZip(workspace: StudioWorkspace, assetFiles: readonly StudioAssetFile[], date = new Date()): Promise<FolderZipArchive> {
  const project = selectedProject(workspace);
  const bot = selectedBot(workspace, project);
  const prefix = `projects/${project.id}/bots/${bot.id}`;
  return buildFolderZip(workspace, assetFiles, prefix, bot.id, bot.id, date);
}

export async function downloadSelectedPackageFolderZip(workspace: StudioWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<void> {
  downloadFolderZip(await buildSelectedPackageFolderZip(workspace, assetFiles));
}

export async function downloadSelectedBotFolderZip(workspace: StudioWorkspace, assetFiles: readonly StudioAssetFile[]): Promise<void> {
  downloadFolderZip(await buildSelectedBotFolderZip(workspace, assetFiles));
}

function selectedPackageFolder(workspace: StudioWorkspace): { pkg: StudioPackage; prefix: string } {
  if (workspace.selectedPackageScope === "shared") {
    const pkg = workspace.shared_packages.find((row) => row.manifest.id === workspace.selectedPackageId);
    if (!pkg) throw new Error(`Shared Package ${workspace.selectedPackageId} does not exist.`);
    return { pkg, prefix: `shared/packages/${pkg.manifest.kind}/${pkg.manifest.id}` };
  }
  const project = selectedProject(workspace);
  if (workspace.selectedPackageScope === "project") {
    const pkg = project.packages.find((row) => row.manifest.id === workspace.selectedPackageId);
    if (!pkg) throw new Error(`Project Package ${workspace.selectedPackageId} does not exist.`);
    return { pkg, prefix: `projects/${project.id}/packages/${pkg.manifest.kind}/${pkg.manifest.id}` };
  }
  const bot = selectedBot(workspace, project);
  if (bot.package.manifest.id !== workspace.selectedPackageId) throw new Error(`Bot Package ${workspace.selectedPackageId} does not exist.`);
  return { pkg: bot.package, prefix: `projects/${project.id}/bots/${bot.id}/package` };
}

async function buildFolderZip(workspace: StudioWorkspace, assetFiles: readonly StudioAssetFile[], prefix: string, root: string, identity: string, date: Date): Promise<FolderZipArchive> {
  const prefixWithSlash = `${prefix}/`;
  const entries = (await encodeContent(workspace, assetFiles))
    .filter((entry) => entry.path.startsWith(prefixWithSlash))
    .map<ZipEntry>((entry) => ({ path: `${root}/${entry.path.slice(prefixWithSlash.length)}`, bytes: base64ToBytes(entry.bytes_base64) }));
  if (entries.length === 0) throw new Error(`Folder ${prefix} has no exportable files.`);
  return {
    filename: `${safeFilename(identity)}-${dateStamp(date)}.zip`,
    mediaType: "application/zip",
    bytes: createZip(entries, date),
  };
}

function downloadFolderZip(archive: FolderZipArchive): void {
  const blob = new Blob([archive.bytes], { type: archive.mediaType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = archive.filename;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function dateStamp(date: Date): string {
  if (!Number.isFinite(date.getTime())) throw new Error("Archive date is invalid.");
  return `${String(date.getFullYear()).padStart(4, "0")}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function safeFilename(value: string): string { return value.replace(/[^a-zA-Z0-9._-]+/gu, "-").replace(/^-+|-+$/gu, "") || "gvya"; }
function base64ToBytes(value: string): Uint8Array { const binary = atob(value); const bytes = new Uint8Array(binary.length); for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index); return bytes; }
