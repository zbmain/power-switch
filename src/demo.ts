import type { AppData, ModelConfig, Settings } from "./types";
import { newModel } from "./types";

const models: ModelConfig[] = [
  {
    ...newModel(),
    id: "demo-1",
    name: "日常搭档",
    modelId: "my-chat-model",
    baseUrl: "https://api.example.com/v1",
    apiKey: "demo-key-not-real",
    supportsImages: true,
  },
  {
    ...newModel(),
    id: "demo-2",
    name: "专注代码",
    protocol: "openai-responses",
    modelId: "my-coding-model",
    baseUrl: "https://coding.example.com/v1",
    apiKey: "demo-key-not-real",
    contextWindow: 128000,
  },
  {
    ...newModel(),
    id: "demo-3",
    name: "灵感工作室",
    protocol: "anthropic-messages",
    modelId: "my-creative-model",
    baseUrl: "https://models.example.com",
    apiKey: "",
    supportsImages: true,
  },
];
let settings: Settings = {
  theme: "system",
  workbuddyPath: null,
  claudePath: null,
  codexDir: null,
};

/** Keep the browser preview useful without pretending to have native file access. */
export async function demoCall(
  command: string,
  args: Record<string, unknown> = {},
): Promise<unknown> {
  if (command === "list_models") {
    const connection = args.connection as ModelConfig;
    return connection.protocol === "anthropic-messages"
      ? ["my-creative-model", "claude-demo", "claude-demo-thinking"]
      : connection.protocol === "openai-responses"
        ? ["my-coding-model", "responses-demo", "responses-demo-mini"]
        : ["my-chat-model", "chat-demo", "chat-demo-mini"];
  }
  if (command === "test_model")
    throw new Error("浏览器演示无法进行真实模型测试，请在桌面应用中测试。");
  if (command === "get_data")
    return structuredClone({
      models,
      settings,
      backups: [],
      dataDir: "桌面端系统应用数据目录",
      agents: [
        {
          agent: "workbuddy",
          path: "用户目录/.workbuddy/models.json",
          exists: false,
        },
        {
          agent: "claude",
          path: "用户目录/.claude/settings.json",
          exists: false,
        },
        { agent: "codex", path: "用户目录/.codex/config.toml", exists: false },
      ],
    } satisfies AppData);
  if (command === "save_model") {
    const model = structuredClone(args.model as ModelConfig);
    if (!model.id) model.id = crypto.randomUUID();
    const i = models.findIndex((m) => m.id === model.id);
    if (i < 0) models.push(model);
    else models[i] = model;
    return model;
  }
  if (command === "delete_model") {
    const i = models.findIndex((m) => m.id === args.id);
    if (i >= 0) models.splice(i, 1);
    return;
  }
  if (command === "save_settings") {
    settings = structuredClone(args.settings as Settings);
    return;
  }
  if (command === "cancel_preview") return;
  throw new Error(
    "浏览器为界面演示。配置写入、备份和链接导入请在 power-switch 桌面应用中使用。",
  );
}
