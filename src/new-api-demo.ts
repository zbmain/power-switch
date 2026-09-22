import { demoCall } from "./demo";
import type { NewApiImport, NewApiImported } from "./new-api-api";
import type { ModelConfig } from "./types";

let connected = false;
const imported = new Map<string, NewApiImported>();

/** Simulate the connector in memory only; browser previews never contact a real New API instance. */
export async function newApiDemo(
  command: string,
  args: Record<string, unknown>,
): Promise<unknown> {
  const baseUrl = String(args.baseUrl ?? "https://new-api.banmahui.cn").replace(
    /\/$/,
    "",
  );
  if (command === "new_api_check")
    return {
      baseUrl,
      version: "v1.0.0-rc.21",
      provider: { name: "Keycloak", slug: "keycloak" },
    };
  if (command === "new_api_login") {
    connected = true;
    return "demo-login";
  }
  if (command === "new_api_cancel_login") return;
  if (command === "new_api_disconnect") {
    connected = false;
    return;
  }
  if (command === "new_api_status")
    return {
      baseUrl,
      phase: connected ? "connected" : "disconnected",
      user: connected
        ? {
            id: 1,
            username: "demo",
            display_name: "演示用户",
            group: "default",
          }
        : null,
      loginId: null,
      error: null,
    };
  if (command === "new_api_catalog")
    return {
      groups: [{ id: "default", label: "默认分组" }],
      selectedGroup: "default",
      models: [
        {
          modelId: "example-chat",
          protocols: ["openai-chat", "anthropic-messages"],
        },
        {
          modelId: "example-responses",
          protocols: ["openai-chat", "openai-responses", "anthropic-messages"],
        },
        {
          modelId: "auto",
          protocols: ["openai-chat", "openai-responses", "anthropic-messages"],
        },
      ],
    };
  if (command === "new_api_import") {
    const request = args.request as NewApiImport;
    const identity = `${request.baseUrl}|${request.group}|${request.modelId}|${request.protocol}`;
    const previous = imported.get(identity);
    const model: ModelConfig = {
      id: previous?.model.id ?? "",
      name: request.name,
      modelId: request.modelId,
      protocol: request.protocol,
      baseUrl:
        request.protocol === "anthropic-messages"
          ? request.baseUrl
          : `${request.baseUrl}/v1`,
      apiKey: "sk-demo-not-a-real-key",
      supportsToolCall: request.supportsToolCall,
      supportsImages: request.supportsImages,
      contextWindow: request.contextWindow,
      reasoningLevels: request.reasoningLevels,
    };
    const saved = (await demoCall("save_model", { model })) as ModelConfig;
    const result = {
      model: saved,
      reused: Boolean(previous),
      message: "演示配置已加入内存模型库；未登录真实服务、未创建真实密钥。",
    };
    imported.set(identity, result);
    return result;
  }
  if (command === "new_api_test")
    return "演示测试完成；未向任何模型发送实际请求，也未消耗额度。";
  throw { code: "demo", message: "演示模式不支持此操作。" };
}
