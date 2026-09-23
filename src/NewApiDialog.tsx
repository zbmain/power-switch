import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  Check,
  Copy,
  Eye,
  EyeOff,
  KeyRound,
  LoaderCircle,
  LogIn,
  RefreshCw,
  ShieldCheck,
  Unplug,
} from "lucide-react";
import { isDesktop } from "./api";
import { Modal } from "./components";
import {
  defaultNewApiUrl,
  newApi,
  newApiError,
  type NewApiCatalog,
  type NewApiConnection,
  type NewApiError,
  type NewApiImported,
  type NewApiStatus,
} from "./new-api-api";
import { agentLabels, type AgentKind, type Protocol } from "./types";
import "./new-api.css";

const agentProtocol: Record<AgentKind, Protocol> = {
  workbuddy: "openai-chat",
  claude: "anthropic-messages",
  codex: "openai-responses",
};
const urlStorageKey = "power-switch.new-api-url";

/** Read only a non-secret instance preference; private-mode storage failures are harmless. */
function savedUrl(): string {
  try {
    return localStorage.getItem(urlStorageKey) || defaultNewApiUrl;
  } catch {
    return defaultNewApiUrl;
  }
}

/** Guide one native login and recoverable model import without exposing management credentials. */
export function NewApiDialog({
  onClose,
  onAdded,
}: {
  onClose: () => void;
  onAdded: () => Promise<void>;
}) {
  const [baseUrl, setBaseUrl] = useState(savedUrl);
  const [connection, setConnection] = useState<NewApiConnection | null>(null);
  const [status, setStatus] = useState<NewApiStatus | null>(null);
  const [catalog, setCatalog] = useState<NewApiCatalog | null>(null);
  const [agent, setAgent] = useState<AgentKind>("workbuddy");
  const [modelId, setModelId] = useState("");
  const [platformName, setPlatformName] = useState("winwin");
  const [context, setContext] = useState("");
  const [tools, setTools] = useState(true);
  const [images, setImages] = useState(true);
  const [reasoning, setReasoning] = useState<string[]>([]);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState<NewApiError | null>(null);
  const [created, setCreated] = useState<NewApiImported | null>(null);
  const [testMessage, setTestMessage] = useState("");
  const [reveal, setReveal] = useState(false);
  const [copied, setCopied] = useState(false);
  const [replaceInvalid, setReplaceInvalid] = useState(false);
  const [restartUncertain, setRestartUncertain] = useState(false);
  const login = useRef<string | null>(null);
  const alive = useRef(true);
  const generation = useRef(0);
  const initialUrl = useRef(baseUrl);
  const protocol = agentProtocol[agent];
  const candidates =
    catalog?.models.filter((model) => model.protocols.includes(protocol)) ?? [];
  const pending = status?.phase === "pending";
  const connected = status?.phase === "connected";

  useEffect(() => {
    alive.current = true;
    let canceled = false;
    setBusy("check");
    void newApi
      .check(initialUrl.current)
      .then(async (info) => {
        const next = await newApi.status(info.baseUrl);
        if (!canceled) {
          setConnection(info);
          setBaseUrl(info.baseUrl);
          setStatus(next);
          if (next.error) setError(next.error);
        }
      })
      .catch((e) => {
        if (!canceled) setError(newApiError(e));
      })
      .finally(() => {
        if (!canceled) setBusy("");
      });
    return () => {
      canceled = true;
      alive.current = false;
      if (login.current) void newApi.cancelLogin(login.current).catch(() => {});
    };
  }, []);

  useEffect(() => {
    if (!pending || !connection) return;
    let canceled = false;
    let timer: ReturnType<typeof setTimeout>;
    /** Poll sequentially so a slow network cannot overlap login checks. */
    async function poll() {
      try {
        const next = await newApi.status(connection!.baseUrl);
        if (canceled) return;
        setStatus(next);
        if (next.phase !== "pending") {
          login.current = null;
          if (next.error) setError(next.error);
          return;
        }
        timer = setTimeout(() => void poll(), 1000);
      } catch (e) {
        if (!canceled) {
          setError(newApiError(e));
          timer = setTimeout(() => void poll(), 3000);
        }
      }
    }
    void poll();
    return () => {
      canceled = true;
      clearTimeout(timer);
    };
  }, [pending, connection]);

  useEffect(() => {
    if (!connected || !status?.user || !connection) return;
    let canceled = false;
    setBusy("catalog");
    setCatalog(null);
    void newApi
      .catalog(connection.baseUrl, status.user.id, "default")
      .then((value) => {
        if (!canceled) setCatalog(value);
      })
      .catch((e) => {
        if (!canceled) setError(newApiError(e));
      })
      .finally(() => {
        if (!canceled) setBusy("");
      });
    return () => {
      canceled = true;
    };
  }, [connected, status?.user?.id, connection]);

  useEffect(() => {
    const choices =
      catalog?.models.filter((model) => model.protocols.includes(protocol)) ??
      [];
    if (!choices.some((model) => model.modelId === modelId))
      setModelId(
        choices.find((model) => model.modelId === "auto")?.modelId ??
          choices[0]?.modelId ??
          "",
      );
  }, [catalog, protocol, modelId]);
  useEffect(() => {
    setContext("");
    setReasoning([]);
    setReplaceInvalid(false);
    setRestartUncertain(false);
  }, [modelId]);

  /** Serialize visible actions and discard late responses after this dialog has closed. */
  async function run(label: string, action: () => Promise<void>) {
    if (busy) return;
    setBusy(label);
    setError(null);
    try {
      await action();
    } catch (e) {
      if (alive.current) setError(newApiError(e));
    } finally {
      if (alive.current) setBusy("");
    }
  }

  /** Clear data associated with the old server before accepting a newly edited URL. */
  function changeUrl(value: string) {
    generation.current += 1;
    setBaseUrl(value);
    setConnection(null);
    setStatus(null);
    setCatalog(null);
    setCreated(null);
    setError(null);
  }

  /** Check the canonical panel address and remember only this non-secret preference. */
  async function check() {
    const turn = generation.current;
    const info = await newApi.check(baseUrl);
    const next = await newApi.status(info.baseUrl);
    if (!alive.current || turn !== generation.current) return;
    setConnection(info);
    setBaseUrl(info.baseUrl);
    setStatus(next);
    if (next.error) setError(next.error);
    try {
      localStorage.setItem(urlStorageKey, info.baseUrl);
    } catch {
      /* Preferences are optional. */
    }
  }

  /** Start a separate IdP window and retain its opaque handle solely for cancellation. */
  async function signIn() {
    if (!connection) return;
    const id = await newApi.login(connection.baseUrl);
    if (!alive.current) {
      await newApi.cancelLogin(id);
      return;
    }
    login.current = id;
    setCatalog(null);
    setCreated(null);
    setStatus({
      baseUrl: connection.baseUrl,
      phase: "pending",
      user: null,
      loginId: id,
      error: null,
    });
  }

  /** Cancel a flow without deleting any model or API key. */
  async function cancelLogin() {
    if (login.current) await newApi.cancelLogin(login.current);
    login.current = null;
    setStatus(null);
  }

  /** Save a platform/model display name while using the fixed default API group. */
  function submit(event: FormEvent) {
    event.preventDefault();
    if (
      !connection ||
      !status?.user ||
      !catalog ||
      !modelId ||
      !platformName.trim()
    )
      return;
    void run("import", async () => {
      const result = await newApi.importModel({
        baseUrl: connection.baseUrl,
        userId: status.user!.id,
        group: "default",
        modelId,
        protocol,
        name: `${platformName.trim()} · ${modelId}`,
        supportsToolCall: tools,
        supportsImages: images,
        contextWindow: context ? Number(context) : null,
        reasoningLevels: reasoning,
        replaceInvalid,
        restartUncertain,
      });
      if (!alive.current) return;
      setCreated(result);
      setTestMessage("");
      setReveal(false);
      setCopied(false);
      setReplaceInvalid(false);
      setRestartUncertain(false);
      await onAdded();
    });
  }

  /** Copy the complete model key only in response to a direct user action. */
  async function copyKey() {
    if (!created) return;
    try {
      await navigator.clipboard.writeText(created.model.apiKey);
      setCopied(true);
    } catch {
      setError({
        code: "clipboard",
        message: "复制失败，请显示密钥后手动复制。",
      });
    }
  }

  /** Keep scrolling a long dialog from silently changing the focused context-window value. */
  function blurNumberOnWheel(event: React.WheelEvent<HTMLInputElement>) {
    event.currentTarget.blur();
  }

  return (
    <Modal
      title="从 New API 添加模型"
      description="连接账号，选择客户端与模型，创建专属密钥。"
      onClose={onClose}
      busy={Boolean(busy)}
      className="new-api-modal"
      wide
    >
      <div className="new-api-dialog">
        {!isDesktop && (
          <div className="warning-note">
            <ShieldCheck size={18} />
            <span>
              浏览器演示：仅使用示例数据，不会登录服务或创建真实密钥。
            </span>
          </div>
        )}
        <div className="new-api-step-heading">
          <span className="new-api-step-number">01</span>
          <div>
            <strong>连接 New API</strong>
            <p>确认实例地址并登录账号，应用会读取你可用的模型。</p>
          </div>
        </div>
        <section className="new-api-section">
          <label className="field">
            New API 实例地址
            <div className="new-api-inline">
              <input
                aria-label="New API 实例地址"
                value={baseUrl}
                disabled={Boolean(busy) || pending}
                onChange={(e) => changeUrl(e.target.value)}
                placeholder={defaultNewApiUrl}
                spellCheck={false}
              />
              <button
                className="button secondary"
                disabled={Boolean(busy) || pending || !baseUrl.trim()}
                onClick={() => void run("check", check)}
              >
                <RefreshCw size={15} />
                {busy === "check" ? "检查中…" : "检查连接"}
              </button>
            </div>
          </label>
          {connection && (
            <p className="field-hint">
              已连接 · {connection.version} · {connection.provider.name}
            </p>
          )}
          <div className="new-api-account">
            <div>
              <strong>
                {connected
                  ? `已登录 · ${status.user?.display_name || status.user?.username}`
                  : pending
                    ? "等待钉钉授权"
                    : "连接你的 New API 账号"}
              </strong>
              <p>
                {connected
                  ? "登录会话保存在系统钥匙串，API Key 单独保存在本机模型库。"
                  : pending
                    ? "请在独立登录窗口中完成扫码；授权完成后将自动返回。"
                    : "首次需要完成扫码。会话过期后可重新登录，已有密钥仍独立有效。"}
              </p>
            </div>
            {pending ? (
              <button
                className="button secondary"
                disabled={Boolean(busy)}
                onClick={() => void run("cancel", cancelLogin)}
              >
                取消登录
              </button>
            ) : (
              <button
                className="button secondary"
                disabled={Boolean(busy) || !connection}
                onClick={() => void run("login", signIn)}
              >
                <LogIn size={16} />
                {connected
                  ? "切换账号"
                  : isDesktop
                    ? "钉钉 / Keycloak 登录"
                    : "演示登录"}
              </button>
            )}
            {connected && (
              <button
                className="icon-button"
                aria-label="断开 New API 连接"
                title="断开连接，保留已导入密钥"
                disabled={Boolean(busy)}
                onClick={() =>
                  void run("disconnect", async () => {
                    await newApi.disconnect(connection!.baseUrl);
                    setStatus(null);
                    setCatalog(null);
                    setCreated(null);
                  })
                }
              >
                <Unplug size={17} />
              </button>
            )}
          </div>
        </section>
        {busy === "catalog" && (
          <p role="status" className="new-api-progress">
            <LoaderCircle size={17} className="spin" />
            正在读取默认分组的可用模型…
          </p>
        )}
        {connected && !catalog && !busy && (
          <button
            className="button secondary"
            onClick={() =>
              void run("catalog", async () =>
                setCatalog(
                  await newApi.catalog(
                    connection!.baseUrl,
                    status.user!.id,
                    "default",
                  ),
                ),
              )
            }
          >
            重新读取模型
          </button>
        )}
        {connected && catalog && !created && (
          <form onSubmit={submit} className="new-api-form">
            <div className="new-api-step-heading">
              <span className="new-api-step-number">02</span>
              <div>
                <strong>选择模型</strong>
              </div>
            </div>
            <div className="new-api-fields">
              <label className="field">
                目前客户端
                <select
                  value={agent}
                  disabled={Boolean(busy)}
                  onChange={(e) => {
                    setAgent(e.target.value as AgentKind);
                    setError(null);
                  }}
                >
                  {Object.entries(agentLabels).map(([value, label]) => (
                    <option key={value} value={value}>
                      {label}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                平台名称
                <input
                  value={platformName}
                  required
                  maxLength={256}
                  disabled={Boolean(busy)}
                  onChange={(e) => setPlatformName(e.target.value)}
                />
              </label>
              <label className="field">
                模型
                <select
                  required
                  value={modelId}
                  disabled={Boolean(busy) || !candidates.length}
                  onChange={(e) => {
                    setModelId(e.target.value);
                    setError(null);
                  }}
                >
                  {!candidates.length && (
                    <option value="">没有兼容此客户端的模型</option>
                  )}
                  {candidates.map((model) => (
                    <option key={model.modelId} value={model.modelId}>
                      {model.modelId}
                    </option>
                  ))}
                </select>
                <span className="field-hint">
                  仅显示 default 分组中支持当前客户端的模型。
                </span>
              </label>
            </div>
            <div className="new-api-step-heading">
              <span className="new-api-step-number">03</span>
              <div>
                <strong>创建专属密钥</strong>
                <p>保存后进入模型库，再选择应用到 Agent。</p>
              </div>
            </div>
            <div className="new-api-policy">
              <KeyRound size={18} />
              <div>
                <strong>专属密钥 · 长期有效</strong>
                <code>
                  {protocol === "anthropic-messages"
                    ? connection?.baseUrl
                    : `${connection?.baseUrl}/v1`}
                </code>
              </div>
            </div>
            <details
              className="advanced"
              open={agent === "codex" ? true : undefined}
            >
              <summary>模型能力设置</summary>
              <div className="advanced-body">
                <p className="field-hint">
                  New API 未提供完整的模型能力信息，请按上游实际能力填写。
                </p>
                <label className="field">
                  上下文窗口（Token）
                  {agent === "codex" && (
                    <span className="optional">Codex 必填</span>
                  )}
                  <input
                    type="number"
                    min={1024}
                    max={100000000}
                    step={1}
                    required={agent === "codex"}
                    value={context}
                    disabled={Boolean(busy)}
                    onWheel={blurNumberOnWheel}
                    onChange={(e) => setContext(e.target.value)}
                    placeholder="按实际模型能力填写"
                  />
                </label>
                <div className="capabilities">
                  <label>
                    <input
                      type="checkbox"
                      checked={tools}
                      disabled={Boolean(busy)}
                      onChange={(e) => setTools(e.target.checked)}
                    />
                    工具调用
                  </label>
                  <label>
                    <input
                      type="checkbox"
                      checked={images}
                      disabled={Boolean(busy)}
                      onChange={(e) => setImages(e.target.checked)}
                    />
                    图像输入
                  </label>
                </div>
                <fieldset className="effort-options">
                  <legend>推理档位（不确定可留空）</legend>
                  {[
                    "none",
                    "minimal",
                    "low",
                    "medium",
                    "high",
                    "xhigh",
                    "max",
                  ].map((level) => (
                    <label key={level}>
                      <input
                        type="checkbox"
                        disabled={Boolean(busy)}
                        checked={reasoning.includes(level)}
                        onChange={(e) =>
                          setReasoning(
                            e.target.checked
                              ? [...reasoning, level]
                              : reasoning.filter((value) => value !== level),
                          )
                        }
                      />
                      {level}
                    </label>
                  ))}
                </fieldset>
              </div>
            </details>
            {error?.code === "token_invalid" && (
              <label className="checkbox-line">
                <input
                  type="checkbox"
                  checked={replaceInvalid}
                  onChange={(e) => setReplaceInvalid(e.target.checked)}
                />
                创建替代密钥并更新本机关联模型（旧密钥保留）
              </label>
            )}
            {error?.code === "creation_uncertain" && (
              <label className="checkbox-line">
                <input
                  type="checkbox"
                  checked={restartUncertain}
                  onChange={(e) => setRestartUncertain(e.target.checked)}
                />
                我已核对 New API 令牌列表，允许新建一把密钥
              </label>
            )}
            <div className="modal-footer">
              <span className="field-hint">保存后，可预览并应用到 Agent。</span>
              <button
                className="button primary"
                disabled={Boolean(busy) || !modelId || !platformName.trim()}
              >
                {busy === "import" ? (
                  <LoaderCircle size={16} className="spin" />
                ) : (
                  <KeyRound size={16} />
                )}{" "}
                {busy === "import"
                  ? "正在创建并保存…"
                  : error?.code === "creation_uncertain" && !restartUncertain
                    ? "重新检查并继续"
                    : "创建并添加"}
              </button>
            </div>
          </form>
        )}
        {created && (
          <section className="new-api-success">
            <div className="new-api-success-title">
              <Check size={20} />
              <div>
                <h3>{created.model.name}</h3>
                <p>{created.reused ? "已复用专属密钥" : "已创建专属密钥"}</p>
              </div>
            </div>
            <p role="status">{created.message}</p>
            <label className="field">
              API Key
              <div className="secret-input">
                <input
                  aria-label="已创建的 API Key"
                  type={reveal ? "text" : "password"}
                  value={created.model.apiKey}
                  readOnly
                  autoComplete="off"
                  spellCheck={false}
                />
                <button
                  className="icon-button"
                  aria-label={reveal ? "隐藏 API Key" : "显示 API Key"}
                  onClick={() => setReveal(!reveal)}
                >
                  {reveal ? <EyeOff size={17} /> : <Eye size={17} />}
                </button>
                <button
                  className="icon-button"
                  aria-label="复制 API Key"
                  onClick={() => void copyKey()}
                >
                  {copied ? <Check size={17} /> : <Copy size={17} />}
                </button>
              </div>
            </label>
            <div className="new-api-test">
              <button
                className="button secondary"
                disabled={Boolean(busy)}
                onClick={() =>
                  void run("test", async () => {
                    setTestMessage("");
                    setTestMessage(await newApi.test(created.model.id));
                  })
                }
              >
                {busy === "test" ? (
                  <LoaderCircle size={16} className="spin" />
                ) : (
                  <RefreshCw size={16} />
                )}
                测试连接
              </button>
              <span className="field-hint">
                会发送一次最小模型请求，消耗少量账户额度。
              </span>
            </div>
            {testMessage && (
              <p role="status" className="new-api-test-result">
                {testMessage}
              </p>
            )}
            <div className="modal-footer">
              <button
                className="button secondary"
                disabled={Boolean(busy)}
                onClick={() => {
                  setCreated(null);
                  setTestMessage("");
                  setError(null);
                }}
              >
                继续添加
              </button>
              <button
                className="button primary"
                disabled={Boolean(busy)}
                onClick={onClose}
              >
                返回模型库
                <Check size={16} />
              </button>
            </div>
          </section>
        )}
        {error && (
          <p role="alert" className="inline-error">
            {error.message}
          </p>
        )}
      </div>
    </Modal>
  );
}
