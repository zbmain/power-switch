import { invoke, isTauri } from "@tauri-apps/api/core";
import type {
  AgentKind,
  AppData,
  ApplyPreview,
  ApplyResult,
  ImportPreview,
  ModelConfig,
  Settings,
} from "./types";

export const isDesktop = isTauri();

/** Execute native IPC; the explicitly labeled browser demo uses volatile sample data only. */
async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isDesktop) return invoke<T>(command, args);
  const { demoCall } = await import("./demo");
  return demoCall(command, args) as Promise<T>;
}

export const api = {
  /** Read the desktop library and current target-file locations. */
  data: () => call<AppData>("get_data"),
  /** Persist a model without applying it to an Agent. */
  save: (model: ModelConfig) => call<ModelConfig>("save_model", { model }),
  /** Delete a model from the library only. */
  delete: (id: string) => call<void>("delete_model", { id }),
  /** Persist validated appearance and path settings. */
  settings: (settings: Settings) => call<void>("save_settings", { settings }),
  /** Generate the second-confirmation preview. */
  preview: (id: string, agents: AgentKind[]) =>
    call<ApplyPreview>("preview_apply", { id, agents }),
  /** Apply only a previously prepared, unmodified preview. */
  apply: (token: string) => call<ApplyResult>("apply_preview", { token }),
  /** Discard canceled secret-bearing projections. */
  cancel: (token: string) => call<void>("cancel_preview", { token }),
  /** Preview restoration of a saved transaction. */
  restore: (id: string) => call<ApplyPreview>("preview_restore", { id }),
  /** Delete a backup only after the separate destructive-action confirmation. */
  deleteBackup: (id: string) => call<void>("delete_backup", { id }),
  /** Decode an import link into safe display metadata. */
  importPreview: (link: string) =>
    call<ImportPreview>("preview_import", { link }),
  /** Save an imported batch, with explicit replacement indexes. */
  importConfirm: (token: string, updates: number[]) =>
    call<number>("confirm_import", { token, updates }),
  /** Export a model URL with an explicit credential policy. */
  share: (id: string, includeSecret: boolean) =>
    call<string>("share_model", { id, includeSecret }),
};
