import { invoke } from "@tauri-apps/api/core";
import { isDesktop } from "./api";
import type { ModelConfig, Protocol } from "./types";

export const defaultNewApiUrl = "https://new-api.banmahui.cn";
export interface NewApiError {
  code: string;
  message: string;
}
export interface NewApiUser {
  id: number;
  username: string;
  display_name: string;
  group: string;
}
export interface NewApiConnection {
  baseUrl: string;
  version: string;
  provider: { name: string; slug: string };
}
export interface NewApiStatus {
  baseUrl: string;
  phase: "disconnected" | "pending" | "connected" | "error";
  user: NewApiUser | null;
  loginId: string | null;
  error: NewApiError | null;
}
export interface NewApiCatalog {
  groups: { id: string; label: string }[];
  selectedGroup: string;
  models: { modelId: string; protocols: Protocol[] }[];
}
export interface NewApiImport {
  baseUrl: string;
  userId: number;
  group: string;
  modelId: string;
  protocol: Protocol;
  name: string;
  supportsToolCall: boolean;
  supportsImages: boolean;
  contextWindow: number | null;
  reasoningLevels: string[];
  replaceInvalid: boolean;
  restartUncertain: boolean;
}
export interface NewApiImported {
  model: ModelConfig;
  reused: boolean;
  message: string;
}

/** Preserve structured native errors; never reflect arbitrary transport objects into the interface. */
export function newApiError(error: unknown): NewApiError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof error.code === "string" &&
    typeof error.message === "string"
  ) {
    return { code: error.code, message: error.message };
  }
  return { code: "unknown", message: "操作未完成，请检查连接后重试。" };
}

/** Keep the web preview explicitly synthetic and execute real credentials only through native IPC. */
async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (isDesktop) return invoke<T>(command, args);
  const { newApiDemo } = await import("./new-api-demo");
  return newApiDemo(command, args ?? {}) as Promise<T>;
}

export const newApi = {
  /** Inspect the supported version and configured identity provider. */
  check: (baseUrl: string) =>
    call<NewApiConnection>("new_api_check", { baseUrl }),
  /** Restore login or poll a pending native authorization. */
  status: (baseUrl: string) =>
    call<NewApiStatus>("new_api_status", { baseUrl }),
  /** Open the isolated login window and return an opaque cancellation handle. */
  login: (baseUrl: string) => call<string>("new_api_login", { baseUrl }),
  /** Cancel a matching login without affecting other instances. */
  cancelLogin: (loginId: string) =>
    call<void>("new_api_cancel_login", { loginId }),
  /** Fetch only the displayed account's available models and protocols. */
  catalog: (baseUrl: string, userId: number, group: string | null = null) =>
    call<NewApiCatalog>("new_api_catalog", { baseUrl, userId, group }),
  /** Create or recover a dedicated key and save its model configuration. */
  importModel: (request: NewApiImport) =>
    call<NewApiImported>("new_api_import", { request }),
  /** Send a minimal paid inference only after a user presses the test button. */
  test: (modelId: string) => call<string>("new_api_test", { modelId }),
  /** Remove this instance's local login while retaining imported API keys. */
  disconnect: (baseUrl: string) =>
    call<void>("new_api_disconnect", { baseUrl }),
};
