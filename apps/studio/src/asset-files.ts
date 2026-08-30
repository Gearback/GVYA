import type { StudioAssetFile, StudioBrainWorkspace, StudioPackage, StudioWorkspace } from "./types.js";

export const STUDIO_ASSET_MAX_BYTES = 32 * 1024 * 1024;
export const STUDIO_ASSET_TOTAL_MAX_BYTES = 128 * 1024 * 1024;

export function assetFileKey(ownerKey: string, source: string): string { return `${ownerKey}\0${source}`; }

export function sharedAssetOwner(packageId: string): string { return `shared:${packageId}`; }
export function projectAssetOwner(projectId: string, packageId: string): string { return `project:${projectId}:${packageId}`; }
export function botAssetOwner(projectId: string, botId: string, packageId: string): string { return `bot:${projectId}:${botId}:${packageId}`; }

export function safeAssetSource(source: string): boolean {
  if (!source.startsWith("assets/") || source.startsWith("/") || source.includes("\\") || source.includes("\0")) return false;
  return source.split("/").every((part) => part !== "" && part !== "." && part !== "..");
}

export function packageAssetPath(pkg: StudioPackage, source: string): string {
  if (!safeAssetSource(source)) throw new Error(`Asset source must be a safe package-relative path under assets/: ${source}`);
  const separator = pkg.path.lastIndexOf("/");
  const base = separator < 0 ? "" : pkg.path.slice(0, separator + 1);
  return `${base}${source}`;
}

export function validateAssetFiles(files: readonly StudioAssetFile[]): StudioAssetFile[] {
  const keys = new Set<string>();
  let total = 0;
  return files.map((file) => {
    if (!file.owner_key.trim()) throw new Error("Asset file owner_key is required.");
    if (!file.package_id.trim()) throw new Error("Asset file package_id is required.");
    if (!safeAssetSource(file.source)) throw new Error(`Asset file source is unsafe: ${file.source}`);
    if (!file.media_type.trim()) throw new Error(`Asset file ${file.source} media_type is required.`);
    if (!(file.blob instanceof Blob)) throw new Error(`Asset file ${file.source} has no binary payload.`);
    if (file.blob.size > STUDIO_ASSET_MAX_BYTES) throw new Error(`Asset file ${file.source} exceeds the 32 MiB Studio limit.`);
    total += file.blob.size;
    if (!Number.isSafeInteger(total) || total > STUDIO_ASSET_TOTAL_MAX_BYTES) throw new Error("Studio asset bytes exceed the 128 MiB workspace limit.");
    const key = assetFileKey(file.owner_key, file.source);
    if (keys.has(key)) throw new Error(`Duplicate Studio asset file ${file.owner_key}/${file.source}.`);
    keys.add(key);
    return { owner_key: file.owner_key, package_id: file.package_id, source: file.source, media_type: file.media_type, blob: file.blob };
  });
}

export function assetFilesForBrain(workspace: StudioBrainWorkspace, ownerKeys: ReadonlySet<string>, files: readonly StudioAssetFile[]): StudioAssetFile[] {
  const packageIds = new Set(workspace.packages.map((pkg) => pkg.manifest.id));
  return files.filter((file) => packageIds.has(file.package_id) && ownerKeys.has(file.owner_key));
}

export function livePackageIds(workspace: { shared_packages: StudioPackage[]; projects: Array<{ packages: StudioPackage[]; bots: Array<{ package: StudioPackage }> }> }): Set<string> {
  return new Set([
    ...workspace.shared_packages.map((pkg) => pkg.manifest.id),
    ...workspace.projects.flatMap((project) => [...project.packages.map((pkg) => pkg.manifest.id), ...project.bots.map((bot) => bot.package.manifest.id)]),
  ]);
}

export function liveAssetOwnerKeys(workspace: StudioWorkspace): Set<string> {
  return new Set([
    ...workspace.shared_packages.map((pkg) => sharedAssetOwner(pkg.manifest.id)),
    ...workspace.projects.flatMap((project) => [
      ...project.packages.map((pkg) => projectAssetOwner(project.id, pkg.manifest.id)),
      ...project.bots.map((bot) => botAssetOwner(project.id, bot.id, bot.package.manifest.id)),
    ]),
  ]);
}

export function selectedBrainAssetOwnerKeys(workspace: StudioWorkspace): Set<string> {
  const project = workspace.projects.find((row) => row.id === workspace.selectedProjectId) ?? null;
  const bot = project?.bots.find((row) => row.id === workspace.selectedBotId) ?? null;
  return new Set([
    ...(project?.packages.map((pkg) => projectAssetOwner(project.id, pkg.manifest.id)) ?? []),
    ...(project && bot ? [botAssetOwner(project.id, bot.id, bot.package.manifest.id)] : []),
  ]);
}

export function packagePreviewAssetOwnerKeys(workspace: StudioWorkspace, preview: StudioBrainWorkspace): Set<string> {
  const packageIds = new Set(preview.packages.map((pkg) => pkg.manifest.id));
  const project = workspace.projects.find((row) => row.id === workspace.selectedProjectId) ?? null;
  const bot = project?.bots.find((row) => row.id === workspace.selectedBotId) ?? null;
  const owners = new Set<string>();
  for (const pkg of workspace.shared_packages) if (packageIds.has(pkg.manifest.id)) owners.add(sharedAssetOwner(pkg.manifest.id));
  for (const pkg of project?.packages ?? []) if (packageIds.has(pkg.manifest.id)) owners.add(projectAssetOwner(project!.id, pkg.manifest.id));
  if (project && bot && packageIds.has(bot.package.manifest.id)) owners.add(botAssetOwner(project.id, bot.id, bot.package.manifest.id));
  if (owners.size !== packageIds.size) throw new Error("Package preview contains an unavailable asset owner.");
  return owners;
}

export function selectedPackageAssetOwner(workspace: StudioWorkspace): string {
  const project = workspace.projects.find((row) => row.id === workspace.selectedProjectId) ?? null;
  if (workspace.selectedPackageScope === "shared") return sharedAssetOwner(workspace.selectedPackageId);
  if (!project) throw new Error("Select a Project before editing package assets.");
  if (workspace.selectedPackageScope === "project") return projectAssetOwner(project.id, workspace.selectedPackageId);
  const bot = project.bots.find((row) => row.id === workspace.selectedBotId) ?? null;
  if (!bot) throw new Error("Select a Bot before editing package assets.");
  return botAssetOwner(project.id, bot.id, workspace.selectedPackageId);
}
