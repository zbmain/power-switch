export type Protocol =
  "openai-chat" | "openai-responses" | "anthropic-messages";
export type AgentKind = "workbuddy" | "claude" | "codex";
export interface ModelConfig {
  id: string;
  name: string;
  protocol: Protocol;
  baseUrl: string;
  modelId: string;
  apiKey: string;
  supportsToolCall: boolean;
  supportsImages: boolean;
  contextWindow: number | null;
  reasoningLevels: string[];
}
export interface ModelTestResult {
  message: string;
  elapsedMs: number;
}
export interface ModelTestState {
  key: string;
  status: "testing" | "passed" | "failed";
  message: string;
  expiresAt?: number;
  dismissed?: boolean;
}

/** Show the same concise success copy everywhere, converting milliseconds to seconds. */
export function modelTestSuccess(result: ModelTestResult): string {
  return "测试通过，耗时" + (result.elapsedMs / 1000).toFixed(2) + " 秒";
}

/** Bind transient results to every draft field; never persist or display this credential-bearing key. */
export function modelTestKey(model: ModelConfig): string {
  return JSON.stringify([
    model.id,
    model.name,
    model.protocol,
    model.baseUrl,
    model.modelId,
    model.apiKey,
    model.supportsToolCall,
    model.supportsImages,
    model.contextWindow,
    model.reasoningLevels,
  ]);
}
export interface Settings {
  theme: "system" | "light" | "dark";
  workbuddyPath: string | null;
  claudePath: string | null;
  codexDir: string | null;
}
export interface AgentPath {
  agent: AgentKind;
  path: string;
  exists: boolean;
}
export interface BackupRecord {
  id: string;
  title: string;
  createdAt: number;
  status: string;
  paths: string[];
}
export interface AppData {
  models: ModelConfig[];
  settings: Settings;
  agents: AgentPath[];
  dataDir: string;
  backups: BackupRecord[];
}
export interface FilePreview {
  path: string;
  before: string;
  after: string;
  fingerprint: string;
}
export interface ApplyPreview {
  token: string;
  title: string;
  files: FilePreview[];
  notices: string[];
}
export interface ApplyResult {
  backupId: string;
  paths: string[];
  message: string;
  workbuddySelection?: string | null;
}
export interface ImportRow {
  index: number;
  name: string;
  protocol: Protocol;
  baseUrl: string;
  modelId: string;
  hasApiKey: boolean;
  duplicate: boolean;
}
export interface ImportPreview {
  token: string;
  rows: ImportRow[];
}
export const protocolLabels: Record<Protocol, string> = {
  "openai-chat": "OpenAI Chat Completions",
  "openai-responses": "OpenAI Responses",
  "anthropic-messages": "Anthropic Messages",
};
export const agentLabels: Record<AgentKind, string> = {
  workbuddy: "WorkBuddy",
  claude: "Claude Code",
  codex: "Codex",
};
export const nativeAgent: Record<Protocol, AgentKind> = {
  "openai-chat": "workbuddy",
  "openai-responses": "codex",
  "anthropic-messages": "claude",
};

/** Return an independent draft with conservative optional model capabilities. */
export function newModel(): ModelConfig {
  return {
    id: "",
    name: "",
    protocol: "openai-chat",
    baseUrl: "",
    modelId: "",
    apiKey: "",
    supportsToolCall: true,
    supportsImages: true,
    contextWindow: null,
    reasoningLevels: [],
  };
}

/** Format a timestamp consistently for a Chinese-language desktop UI. */
export function formatTime(seconds: number): string {
  return new Date(seconds * 1000).toLocaleString("zh-CN", { hour12: false });
}
