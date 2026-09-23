import * as Dialog from "@radix-ui/react-dialog";
import {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type ReactNode,
} from "react";
import { api } from "./api";
import {
  ChevronDown,
  Eye,
  EyeOff,
  X,
  ArrowRight,
  AlertTriangle,
  Check,
  FileJson,
  ShieldCheck,
  FlaskConical,
  LoaderCircle,
} from "lucide-react";
import {
  agentLabels,
  nativeAgent,
  protocolLabels,
  modelTestKey,
  modelTestSuccess,
  type AgentKind,
  type ApplyPreview,
  type ModelConfig,
  type Protocol,
  type ModelTestResult,
  type ModelTestState,
} from "./types";

/** Provide a focus-trapped, keyboard-accessible modal with a consistent closing affordance. */
export function Modal({
  title,
  description,
  children,
  onClose,
  wide = false,
  busy = false,
  className = "",
}: {
  title: string;
  description: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
  busy?: boolean;
  className?: string;
}) {
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content
          className={`modal ${wide ? "modal-wide" : ""} ${className}`}
          onInteractOutside={(e) => e.preventDefault()}
        >
          <div className="modal-heading">
            <div>
              <Dialog.Title>{title}</Dialog.Title>
              <Dialog.Description>{description}</Dialog.Description>
            </div>
            <button
              className="icon-button"
              aria-label="关闭弹窗"
              onClick={onClose}
              disabled={busy}
            >
              <X size={20} />
            </button>
          </div>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/** Show the platform name's first character, uppercasing English initials and preserving Chinese characters. */
export function ProtocolMark({
  protocol,
  name,
  small = false,
}: {
  protocol: Protocol;
  name: string;
  small?: boolean;
}) {
  const initial = Array.from(name.trim())[0] || "?";
  return (
    <span
      className={`protocol-mark ${protocol} ${small ? "small" : ""}`}
      aria-hidden="true"
    >
      {/^[a-z]$/.test(initial) ? initial.toUpperCase() : initial}
    </span>
  );
}

/** Dismiss result text after five seconds without clearing the model's verification state. */
export function ModelTestFeedback({
  probe,
  onDismiss,
  className = "",
}: {
  probe: ModelTestState;
  onDismiss: () => void;
  className?: string;
}) {
  const dismiss = useRef(onDismiss);
  const current = useRef(probe);
  dismiss.current = onDismiss;
  current.current = probe;
  useEffect(() => {
    if (probe.dismissed || probe.status === "testing") return;
    const remaining = Math.max(
      0,
      (probe.expiresAt ?? Date.now() + 5000) - Date.now(),
    );
    const timer = window.setTimeout(() => {
      if (current.current === probe) dismiss.current();
    }, remaining);
    return () => window.clearTimeout(timer);
  }, [probe]);
  if (
    probe.dismissed ||
    (probe.expiresAt !== undefined && Date.now() >= probe.expiresAt)
  )
    return null;
  return (
    <div
      className={"model-test-feedback " + probe.status + " " + className}
      role={probe.status === "failed" ? "alert" : "status"}
    >
      <span>{probe.message}</span>
      <button
        type="button"
        className="icon-button"
        aria-label="关闭测试提示"
        title="关闭提示"
        onClick={onDismiss}
      >
        <X size={14} />
      </button>
    </div>
  );
}

/** Accept only complete HTTP(S) base URLs before enabling the model-list request. */
function validCatalogUrl(raw: string): boolean {
  try {
    const url = new URL(raw.trim());
    return (
      ["http:", "https:"].includes(url.protocol) &&
      !!url.hostname &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash
    );
  } catch {
    return false;
  }
}

/** Edit model fields locally; nothing is persisted until the validated form is submitted. */
export function ModelForm({
  initial,
  onSave,
  onCancel,
  busy,
}: {
  initial: ModelConfig;
  onSave: (m: ModelConfig, result: ModelTestResult) => void;
  onCancel: () => void;
  busy: boolean;
}) {
  const [model, setModel] = useState(initial);
  const [reveal, setReveal] = useState(false);
  const [error, setError] = useState("");
  const [probe, setProbe] = useState<
    (ModelTestState & { result?: ModelTestResult }) | null
  >(null);
  const [testing, setTesting] = useState(false);
  const editing = !!initial.id;
  const [catalog, setCatalog] = useState<string[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [catalogError, setCatalogError] = useState("");
  const catalogVersion = useRef(0);
  const selectable =
    !!model.apiKey.trim() && !catalogLoading && catalog.includes(model.modelId);
  const draft = editing
    ? model
    : {
        ...model,
        name:
          model.name.trim() && model.modelId.trim()
            ? `${model.name.trim()} · ${model.modelId.trim()}`
            : model.name.trim(),
      };
  const requestVersion = useRef(0);
  const pending = useRef(false);
  const mounted = useRef(false);
  const form = useRef<HTMLFormElement>(null);
  const canSave =
    selectable &&
    !testing &&
    probe?.status === "passed" &&
    probe.key === modelTestKey(draft) &&
    !!probe.result;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      requestVersion.current += 1;
      catalogVersion.current += 1;
    };
  }, []);
  useEffect(() => {
    if (editing && initial.apiKey.trim()) void fetchModels(initial);
  }, []);
  /** Load the provider catalog and ignore results invalidated by connection edits or unmounting. */
  async function fetchModels(connection = model) {
    if (!connection.apiKey.trim()) {
      setCatalogError("请先填写 API Key");
      return;
    }
    if (!validCatalogUrl(connection.baseUrl)) {
      setCatalogError("请输入有效的 API 地址");
      return;
    }
    const version = ++catalogVersion.current;
    setCatalogLoading(true);
    setCatalog([]);
    setCatalogError("");
    setProbe(null);
    requestVersion.current += 1;
    try {
      const ids = await api.listModels({
        protocol: connection.protocol,
        baseUrl: connection.baseUrl,
        apiKey: connection.apiKey,
      });
      if (mounted.current && version === catalogVersion.current) {
        setCatalog(ids);
        if (!ids.length) setCatalogError("平台未返回可选模型");
      }
    } catch (e) {
      if (mounted.current && version === catalogVersion.current)
        setCatalogError(String(e));
    } finally {
      if (mounted.current && version === catalogVersion.current)
        setCatalogLoading(false);
    }
  }
  /** Update one typed field while keeping every other draft value intact. */
  function update<K extends keyof ModelConfig>(key: K, value: ModelConfig[K]) {
    if (["protocol", "baseUrl", "apiKey"].includes(key)) {
      catalogVersion.current += 1;
      setCatalog([]);
      setCatalogLoading(false);
      setCatalogError(
        key === "apiKey" && !String(value).trim()
          ? "请先填写 API Key"
          : editing || catalog.length || model.modelId
            ? "连接配置已更改，请刷新模型清单"
            : "",
      );
    }
    requestVersion.current += 1;
    setProbe(null);
    setError("");
    setModel((old) => ({
      ...old,
      [key]: value,
      ...(["protocol", "baseUrl", "apiKey"].includes(key)
        ? { modelId: "" }
        : {}),
    }));
  }
  /** Validate essential draft fields before any network request or save. */
  function validDraft(): boolean {
    if (!selectable) return false;
    if (!form.current?.reportValidity()) return false;
    try {
      const url = new URL(model.baseUrl);
      if (!["http:", "https:"].includes(url.protocol)) throw new Error();
    } catch {
      setError("请输入有效的 HTTP 或 HTTPS 地址");
      return false;
    }
    if (!draft.name.trim() || !model.modelId.trim()) {
      setError(editing ? "请填写名称和模型 ID" : "请填写平台名称并选择模型 ID");
      return false;
    }
    if (new TextEncoder().encode(draft.name).length > 256) {
      setError("平台名称与模型 ID 组合后超过长度限制");
      return false;
    }
    return true;
  }
  /** Test a snapshot once and ignore responses superseded by field edits or closing the form. */
  async function testDraft() {
    if (busy || pending.current || !validDraft()) return;
    pending.current = true;
    setTesting(true);
    setError("");
    const version = ++requestVersion.current;
    const snapshot = structuredClone(draft);
    const key = modelTestKey(snapshot);
    setProbe({
      key,
      status: "testing",
      message: "正在发送 test，最长等待 30 秒…",
    });
    try {
      const result = await api.testModel(snapshot);
      if (mounted.current && version === requestVersion.current)
        setProbe({
          key,
          status: "passed",
          message: modelTestSuccess(result),
          expiresAt: Date.now() + 5000,
          result,
        });
    } catch (e) {
      if (mounted.current && version === requestVersion.current)
        setProbe({
          key,
          status: "failed",
          message: String(e),
          expiresAt: Date.now() + 5000,
        });
    } finally {
      pending.current = false;
      if (mounted.current) setTesting(false);
    }
  }
  /** Require a successful test for this exact draft even for Enter-key or programmatic submits. */
  function submit(event: FormEvent) {
    event.preventDefault();
    if (busy || !canSave || !probe?.result || !validDraft()) return;
    onSave(draft, probe.result);
  }
  return (
    <form ref={form} onSubmit={submit} className="model-form">
      <div className="form-grid">
        <label className="field">
          {editing ? "名称" : "平台名称"}
          <input
            autoFocus
            value={model.name}
            maxLength={256}
            onChange={(e) => update("name", e.target.value)}
            placeholder={editing ? "给模型起个好记的名字" : "例如 winwin"}
            required
          />
        </label>
        <label className="field">
          协议
          <select
            value={model.protocol}
            onChange={(e) => update("protocol", e.target.value as Protocol)}
          >
            {Object.entries(protocolLabels).map(([value, label]) => (
              <option key={value} value={value}>
                {label}
              </option>
            ))}
          </select>
        </label>
      </div>
      <label className="field">
        API 地址
        <input
          value={model.baseUrl}
          onChange={(e) => update("baseUrl", e.target.value)}
          placeholder={
            model.protocol === "anthropic-messages"
              ? "https://api.example.com"
              : "https://api.example.com/v1"
          }
          autoCapitalize="off"
          spellCheck={false}
          required
        />
        <span className="field-hint">
          填写 API 基础地址，也可以粘贴标准接口的完整地址。
        </span>
      </label>
      <label className="field">
        API Key
        <div className="secret-input">
          <input
            type={reveal ? "text" : "password"}
            value={model.apiKey}
            onChange={(e) => update("apiKey", e.target.value)}
            placeholder="sk-…"
            autoComplete="off"
            spellCheck={false}
            required
          />
          <button
            type="button"
            className="icon-button"
            aria-label={reveal ? "隐藏密钥" : "显示密钥"}
            onClick={() => setReveal(!reveal)}
          >
            {reveal ? <EyeOff size={17} /> : <Eye size={17} />}
          </button>
        </div>
        <span className="field-hint">
          填写 API Key 后获取平台模型清单。密钥仅保存在本机。
        </span>
      </label>
      <label className="field">
        模型 ID
        <select
          aria-label="模型 ID"
          value={selectable ? model.modelId : ""}
          disabled={catalogLoading || !catalog.length || !model.apiKey.trim()}
          required
          onChange={(e) => update("modelId", e.target.value)}
        >
          <option value="" disabled>
            {catalogLoading ? "正在获取模型清单…" : "请选择平台返回的模型"}
          </option>
          {catalog.map((id) => (
            <option key={id} value={id}>
              {id}
            </option>
          ))}
        </select>
      </label>
      <div className="catalog-controls">
        <button
          type="button"
          className="button secondary"
          disabled={
            busy ||
            catalogLoading ||
            !model.apiKey.trim() ||
            !validCatalogUrl(model.baseUrl)
          }
          onClick={() => void fetchModels()}
        >
          {catalogLoading
            ? "正在获取…"
            : editing || catalog.length || catalogError
              ? "刷新模型清单"
              : "获取模型清单"}
        </button>
        {catalogError && (
          <span role="alert" className="field-hint">
            {catalogError}
          </span>
        )}
        {!catalogLoading && !catalogError && !catalog.length && (
          <span className="field-hint">
            {model.apiKey.trim() ? "获取平台可选模型。" : "请先填写 API Key。"}
          </span>
        )}
        {!catalogLoading &&
          !catalogError &&
          !!model.modelId &&
          !catalog.includes(model.modelId) && (
            <span className="field-hint">
              当前模型 {model.modelId} 不在平台清单中，请重新选择。
            </span>
          )}
      </div>
      <details
        className="advanced"
        open={model.protocol === "openai-responses" ? true : undefined}
      >
        <summary>
          <span>高级设置</span>
          <ChevronDown size={16} />
        </summary>
        <div className="advanced-body">
          <div className="capabilities">
            <label>
              <input
                type="checkbox"
                checked={model.supportsToolCall}
                onChange={(e) => update("supportsToolCall", e.target.checked)}
              />
              工具调用
            </label>
            <label>
              <input
                type="checkbox"
                checked={model.supportsImages}
                onChange={(e) => update("supportsImages", e.target.checked)}
              />
              图像输入
            </label>
          </div>
          <label className="field">
            上下文窗口（Token）
            <input
              type="number"
              min={1024}
              max={100000000}
              step={1}
              value={model.contextWindow ?? ""}
              onChange={(e) =>
                update(
                  "contextWindow",
                  e.target.value ? Number(e.target.value) : null,
                )
              }
              placeholder="例如 128000"
            />
            <span className="field-hint">
              应用到 Codex 时必填，请按实际模型能力填写。
            </span>
          </label>
          <fieldset className="effort-options">
            <legend>
              模型支持的推理档位{" "}
              <span className="optional">不确定可全部留空</span>
            </legend>
            {["none", "minimal", "low", "medium", "high", "xhigh", "max"].map(
              (level) => (
                <label key={level}>
                  <input
                    type="checkbox"
                    checked={model.reasoningLevels.includes(level)}
                    onChange={(e) =>
                      update(
                        "reasoningLevels",
                        e.target.checked
                          ? [...model.reasoningLevels, level]
                          : model.reasoningLevels.filter((x) => x !== level),
                      )
                    }
                  />
                  {level}
                </label>
              ),
            )}
          </fieldset>
        </div>
      </details>
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      {probe ? (
        <ModelTestFeedback
          probe={probe}
          onDismiss={() =>
            setProbe((old) => (old ? { ...old, dismissed: true } : old))
          }
        />
      ) : (
        <p className="model-test-feedback" role="status">
          {testing
            ? "配置已修改，当前请求结束后请重新测试。"
            : "请先测试当前配置，通过后才能保存。测试将发送文本 test。"}
        </p>
      )}
      <div className="modal-footer">
        <button
          type="button"
          className="button secondary"
          onClick={onCancel}
          disabled={busy}
        >
          取消
        </button>
        <button
          type="button"
          className="button secondary"
          disabled={busy || testing || !selectable}
          onClick={() => void testDraft()}
        >
          {testing ? (
            <LoaderCircle size={16} className="spin" />
          ) : (
            <FlaskConical size={16} />
          )}
          {testing ? "测试中…" : "测试模型"}
        </button>
        <button
          className="button primary"
          disabled={busy || !canSave}
          title={
            canSave ? "保存已测试的模型配置" : "当前配置测试通过后才能保存"
          }
        >
          {busy ? "保存中…" : "保存模型"}
          <Check size={16} />
        </button>
      </div>
    </form>
  );
}

/** Select native-compatible targets; incompatible protocols are never submitted. */
export function AgentPicker({
  model,
  onPreview,
  busy,
}: {
  model: ModelConfig;
  onPreview: (agents: AgentKind[], selectWorkbuddyModel: boolean) => void;
  busy: boolean;
}) {
  const [selected, setSelected] = useState<AgentKind[]>([
    nativeAgent[model.protocol],
  ]);
  const [selectWorkbuddyModel, setSelectWorkbuddyModel] = useState(true);
  return (
    <>
      <div className="selected-model">
        <ProtocolMark protocol={model.protocol} name={model.name} small />
        <div>
          <strong>{model.name}</strong>
          <span>{model.modelId}</span>
        </div>
      </div>
      <div className="agent-options">
        {(Object.keys(agentLabels) as AgentKind[]).map((agent) => {
          const compatible = nativeAgent[model.protocol] === agent;
          return (
            <div key={agent}>
              <label
                className={`agent-option ${!compatible ? "disabled" : ""}`}
              >
                <input
                  type="checkbox"
                  checked={selected.includes(agent)}
                  disabled={!compatible || busy}
                  onChange={(e) => {
                    setSelected(
                      e.target.checked
                        ? [...selected, agent]
                        : selected.filter((a) => a !== agent),
                    );
                    if (agent === "workbuddy")
                      setSelectWorkbuddyModel(e.target.checked);
                  }}
                />
                <span>
                  <strong>{agentLabels[agent]}</strong>
                  <small>
                    {compatible
                      ? agent === "workbuddy"
                        ? "加入模型列表，可在新任务中选择"
                        : agent === "codex"
                          ? "CLI 与桌面端共享配置"
                          : "写入用户级模型配置"
                      : "与当前模型协议不兼容"}
                  </small>
                </span>
                {compatible && <span className="tag green">兼容</span>}
              </label>
              {agent === "workbuddy" && compatible && (
                <label className="workbuddy-select-option">
                  <input
                    type="checkbox"
                    checked={
                      selectWorkbuddyModel && selected.includes("workbuddy")
                    }
                    disabled={!selected.includes("workbuddy") || busy}
                    onChange={(e) => setSelectWorkbuddyModel(e.target.checked)}
                  />
                  <span>
                    <strong>写入后打开 WorkBuddy 新建任务页</strong>
                    <small>
                      WorkBuddy 5.6.2
                      已接收但未执行自动选模链接；请在新任务中手动选择此模型，之后会记住该选择。
                    </small>
                  </span>
                </label>
              )}
            </div>
          );
        })}
      </div>
      <div className="info-note">
        <ShieldCheck size={18} />
        <span>下一步查看变更。二次确认后，才会备份并写入配置。</span>
      </div>
      <div className="modal-footer">
        <button
          className="button primary"
          disabled={busy || !selected.length}
          onClick={() =>
            onPreview(
              selected,
              selected.includes("workbuddy") && selectWorkbuddyModel,
            )
          }
        >
          {busy ? "读取配置…" : "预览变更"}
          <ArrowRight size={16} />
        </button>
      </div>
    </>
  );
}

/** Present redacted before/after data as the only route to a destructive write. */
export function ApplyReview({
  preview,
  onConfirm,
  onCancel,
  busy,
}: {
  preview: ApplyPreview;
  onConfirm: () => void;
  onCancel: () => void;
  busy: boolean;
}) {
  return (
    <>
      <div className="warning-note">
        <AlertTriangle size={20} />
        <div>
          <strong>二次确认：将覆盖以下配置文件中的相关内容</strong>
          <p>确认前会自动创建备份。请核对路径与变更，密钥已隐藏。</p>
        </div>
      </div>
      <div className="preview-files">
        {preview.files.map((file) => (
          <details key={file.path} open>
            <summary>
              <FileJson size={16} />
              <span>{file.path}</span>
              <ChevronDown size={15} />
            </summary>
            <div className="diff-grid">
              <div>
                <span className="diff-label">变更前</span>
                <pre>{file.before}</pre>
              </div>
              <div>
                <span className="diff-label after">变更后</span>
                <pre>{file.after}</pre>
              </div>
            </div>
          </details>
        ))}
      </div>
      {preview.notices.map((n) => (
        <p className="preview-notice" key={n}>
          {n}
        </p>
      ))}
      <div className="modal-footer">
        <button className="button secondary" disabled={busy} onClick={onCancel}>
          取消
        </button>
        <button className="button primary" disabled={busy} onClick={onConfirm}>
          {busy ? "正在备份并写入…" : "确认覆盖并备份"}
          <Check size={16} />
        </button>
      </div>
    </>
  );
}
