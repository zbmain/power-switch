import * as Dialog from "@radix-ui/react-dialog";
import { useState, type FormEvent, type ReactNode } from "react";
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
} from "lucide-react";
import {
  agentLabels,
  nativeAgent,
  protocolLabels,
  type AgentKind,
  type ApplyPreview,
  type ModelConfig,
  type Protocol,
} from "./types";

/** Provide a focus-trapped, keyboard-accessible modal with a consistent closing affordance. */
export function Modal({
  title,
  description,
  children,
  onClose,
  wide = false,
  busy = false,
}: {
  title: string;
  description: string;
  children: ReactNode;
  onClose: () => void;
  wide?: boolean;
  busy?: boolean;
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
          className={`modal ${wide ? "modal-wide" : ""}`}
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

/** Render a protocol mark without depending on another product's trademark artwork. */
export function ProtocolMark({
  protocol,
  small = false,
}: {
  protocol: Protocol;
  small?: boolean;
}) {
  const letters: Record<Protocol, string> = {
    "openai-chat": "C",
    "openai-responses": "R",
    "anthropic-messages": "A",
  };
  return (
    <span
      className={`protocol-mark ${protocol} ${small ? "small" : ""}`}
      aria-hidden="true"
    >
      {letters[protocol]}
    </span>
  );
}

/** Edit model fields locally; nothing is persisted until the validated form is submitted. */
export function ModelForm({
  initial,
  onSave,
  onCancel,
  busy,
}: {
  initial: ModelConfig;
  onSave: (m: ModelConfig) => void;
  onCancel: () => void;
  busy: boolean;
}) {
  const [model, setModel] = useState(initial);
  const [reveal, setReveal] = useState(false);
  const [error, setError] = useState("");
  /** Update one typed field while keeping every other draft value intact. */
  function update<K extends keyof ModelConfig>(key: K, value: ModelConfig[K]) {
    setModel((old) => ({ ...old, [key]: value }));
  }
  /** Validate essential fields before passing the draft to Rust's authoritative validation. */
  function submit(event: FormEvent) {
    event.preventDefault();
    try {
      const url = new URL(model.baseUrl);
      if (!["http:", "https:"].includes(url.protocol)) throw new Error();
    } catch {
      setError("请输入有效的 HTTP 或 HTTPS 地址");
      return;
    }
    if (!model.name.trim() || !model.modelId.trim()) {
      setError("请填写名称和模型 ID");
      return;
    }
    onSave(model);
  }
  return (
    <form onSubmit={submit} className="model-form">
      <div className="form-grid">
        <label className="field">
          名称
          <input
            autoFocus
            value={model.name}
            maxLength={256}
            onChange={(e) => update("name", e.target.value)}
            placeholder="给模型起个好记的名字"
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
        模型 ID
        <input
          value={model.modelId}
          onChange={(e) => update("modelId", e.target.value)}
          placeholder="服务商提供的精确模型标识"
          spellCheck={false}
          required
        />
      </label>
      <label className="field">
        API Key <span className="optional">可选</span>
        <div className="secret-input">
          <input
            type={reveal ? "text" : "password"}
            value={model.apiKey}
            onChange={(e) => update("apiKey", e.target.value)}
            placeholder="sk-…"
            autoComplete="off"
            spellCheck={false}
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
          密钥仅保存在本机。留空可用于无需认证的本地服务。
        </span>
      </label>
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
      <div className="modal-footer">
        <button
          type="button"
          className="button secondary"
          onClick={onCancel}
          disabled={busy}
        >
          取消
        </button>
        <button className="button primary" disabled={busy}>
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
  onPreview: (agents: AgentKind[]) => void;
  busy: boolean;
}) {
  const [selected, setSelected] = useState<AgentKind[]>([
    nativeAgent[model.protocol],
  ]);
  return (
    <>
      <div className="selected-model">
        <ProtocolMark protocol={model.protocol} small />
        <div>
          <strong>{model.name}</strong>
          <span>{model.modelId}</span>
        </div>
      </div>
      <div className="agent-options">
        {(Object.keys(agentLabels) as AgentKind[]).map((agent) => {
          const compatible = nativeAgent[model.protocol] === agent;
          return (
            <label
              key={agent}
              className={`agent-option ${!compatible ? "disabled" : ""}`}
            >
              <input
                type="checkbox"
                checked={selected.includes(agent)}
                disabled={!compatible || busy}
                onChange={(e) =>
                  setSelected(
                    e.target.checked
                      ? [...selected, agent]
                      : selected.filter((a) => a !== agent),
                  )
                }
              />
              <span>
                <strong>{agentLabels[agent]}</strong>
                <small>
                  {compatible
                    ? agent === "workbuddy"
                      ? "加入模型列表，在新会话中手动选择"
                      : agent === "codex"
                        ? "CLI 与桌面端共享配置"
                        : "写入用户级模型配置"
                    : "与当前模型协议不兼容"}
                </small>
              </span>
              {compatible && <span className="tag green">兼容</span>}
            </label>
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
          onClick={() => onPreview(selected)}
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
