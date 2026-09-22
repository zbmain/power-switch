import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./api";

export type SkillScope = "global" | "claude" | "codex" | "workbuddy";
export const scopeLabels: Record<SkillScope, string> = {
  global: "全局",
  claude: "Claude Code",
  codex: "Codex",
  workbuddy: "WorkBuddy",
};
export interface SkillEntry {
  id: string;
  name: string;
  description: string;
  path: string;
  linked: boolean;
  linkTarget: string | null;
  problem: string | null;
}
export interface SkillGroup {
  scope: SkillScope;
  path: string;
  skills: SkillEntry[];
  error: string | null;
}
export interface SkillDetail {
  document: string;
  path: string;
}
export interface InstallCandidate {
  index: number;
  name: string;
  description: string;
  folder: string;
  exists: boolean;
}
export interface InstallPreview {
  token: string;
  destination: string;
  skills: InstallCandidate[];
}
export interface SkillChangePreview {
  token: string;
  title: string;
  paths: string[];
  message: string;
}
export type InstallSource =
  | { kind: "local"; path: string }
  | { kind: "git"; url: string; subdir: string | null };

/** Keep browser presentation separate from native skill filesystem operations. */
async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isDesktop) return invoke<T>(command, args);
  const { demoSkills } = await import("./skills-demo");
  return demoSkills(command, args) as Promise<T>;
}

export const skillsApi = {
  /** Rescan all scopes without mutating their contents. */
  list: () => call<SkillGroup[]>("skills_list"),
  /** Load one document for the detail pane. */
  detail: (scope: SkillScope, id: string) =>
    call<SkillDetail>("skills_detail", { scope, id }),
  /** Stage an installation for review; the destination is always global. */
  installPreview: (source: InstallSource) =>
    call<InstallPreview>("skills_preview_install", { source }),
  /** Prepare global-to-Agent soft links without overwriting existing entries. */
  linkPreview: (id: string, scopes: SkillScope[]) =>
    call<SkillChangePreview>("skills_preview_link", { id, scopes }),
  /** Prepare deletion, including links that would otherwise become broken. */
  deletePreview: (scope: SkillScope, id: string) =>
    call<SkillChangePreview>("skills_preview_delete", { scope, id }),
  /** Commit a reviewed, fingerprint-checked operation exactly once. */
  commit: (token: string, selected: number[] = []) =>
    call<string>("skills_commit", { token, selected }),
  /** Release staged files when a dialog is canceled. */
  cancel: (token: string) => call<void>("skills_cancel", { token }),
};
