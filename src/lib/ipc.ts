import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface LaunchSpec {
  command: string;
  args: string[];
  cwd: string | null;
  env: Record<string, string>;
  profile: string | null;
}

export interface ProfileSummary {
  name: string;
  command: string;
  args: string[];
}

export interface ForegroundInfo {
  pid: number;
  name: string;
  cmd: string[];
  cwd: string | null;
  is_ssh: boolean;
}

export interface SpawnPaneResult {
  pane_id: string;
  spec: LaunchSpec;
}

export interface ResolveSplitSpecResult {
  spec: LaunchSpec;
  inherited_ssh: boolean;
  source_foreground: ForegroundInfo | null;
}

export type SpawnSource =
  | { kind: "initial" }
  | { kind: "profile"; name: string }
  | { kind: "spec"; spec: LaunchSpec };

export function getInitialLaunch(): Promise<LaunchSpec> {
  return invoke("get_initial_launch");
}

export function listProfiles(): Promise<ProfileSummary[]> {
  return invoke("list_profiles");
}

export function spawnPane(
  source: SpawnSource,
  cols: number,
  rows: number,
): Promise<SpawnPaneResult> {
  return invoke("spawn_pane", { args: { source, cols, rows } });
}

export function writePane(paneId: string, data: string): Promise<void> {
  return invoke("write_pane", { args: { pane_id: paneId, data } });
}

export function resizePane(
  paneId: string,
  cols: number,
  rows: number,
): Promise<void> {
  return invoke("resize_pane", { args: { pane_id: paneId, cols, rows } });
}

export function closePane(paneId: string): Promise<void> {
  return invoke("close_pane", { args: { pane_id: paneId } });
}

export function paneForegroundInfo(
  paneId: string,
): Promise<ForegroundInfo | null> {
  return invoke("pane_foreground_info", { args: { pane_id: paneId } });
}

export function resolveSplitSpec(
  sourcePaneId: string,
  fallbackProfile: string | null,
): Promise<ResolveSplitSpecResult> {
  return invoke("resolve_split_spec", {
    args: {
      source_pane_id: sourcePaneId,
      fallback_profile: fallbackProfile,
    },
  });
}

// Events ----------------------------------------------------------------------

export interface PaneDataEvent {
  pane_id: string;
  chunk: string;
}

export interface PaneExitEvent {
  pane_id: string;
  code: number | null;
}

export function onPaneData(
  cb: (e: PaneDataEvent) => void,
): Promise<UnlistenFn> {
  return listen<PaneDataEvent>("pane:data", (event) => cb(event.payload));
}

export function onPaneExit(
  cb: (e: PaneExitEvent) => void,
): Promise<UnlistenFn> {
  return listen<PaneExitEvent>("pane:exit", (event) => cb(event.payload));
}
