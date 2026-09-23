import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";
import {
  ArrowDownToLine,
  ArrowRight,
  Check,
  CheckCircle2,
  CircleHelp,
  Copy,
  Download,
  ExternalLink,
  FileClock,
  FlaskConical,
  FolderCog,
  FolderOpen,
  BookOpen,
  Layers3,
  Link2,
  LoaderCircle,
  Monitor,
  Moon,
  Pencil,
  Plus,
  Search,
  Settings2,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  Sun,
  Trash2,
  X,
} from "lucide-react";
import { api, isDesktop } from "./api";
import { pickLocalPath } from "./file-picker";
import { SkillsPage } from "./SkillsPage";
import { NewApiDialog } from "./NewApiDialog";
import { NewcomerGuide } from "./NewcomerGuide";
import {
  AgentPicker,
  ApplyReview,
  Modal,
  ModelForm,
  ModelTestFeedback,
  ProtocolMark,
} from "./components";
import {
  agentLabels,
  formatTime,
  nativeAgent,
  newModel,
  modelTestKey,
  modelTestSuccess,
  protocolLabels,
  type AppData,
  type ApplyPreview,
  type BackupRecord,
  type ImportPreview,
  type ModelConfig,
  type ModelTestState,
  type Settings,
} from "./types";

type Page = "models" | "backups" | "settings" | "skills";
type ModalState =
  | { kind: "guide" }
  | { kind: "new-api" }
  | { kind: "model"; model: ModelConfig }
  | { kind: "agents"; model: ModelConfig }
  | { kind: "review"; preview: ApplyPreview }
  | { kind: "import" }
  | { kind: "import-review"; preview: ImportPreview }
  | { kind: "share"; model: ModelConfig }
  | { kind: "delete"; model: ModelConfig }
  | { kind: "delete-backup"; backup: BackupRecord }
  | null;
type Notice = { kind: "success" | "error"; text: string } | null;

/** Coordinate desktop state and the explicitly confirmed model/apply/import workflows. */
export default function App() {
  const [data, setData] = useState<AppData | null>(null);
  const [themePreview, setThemePreview] = useState<Settings["theme"] | null>(
    null,
  );
  const theme = themePreview ?? data?.settings.theme ?? "system";
  const [page, setPage] = useState<Page>("models");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState("all");
  const [modal, setModal] = useState<ModalState>(null);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const [modelTests, setModelTests] = useState<Record<string, ModelTestState>>(
    {},
  );
  const pendingTests = useRef(new Map<string, symbol>());
  const [links, setLinks] = useState<string[]>([]);
  const seenLinks = useRef(new Map<string, number>());
  /** Reload authoritative native state following a successful mutation. */
  const refresh = useCallback(async () => {
    setData(await api.data());
  }, []);
  useEffect(() => {
    void refresh().catch((e) => setNotice({ kind: "error", text: String(e) }));
  }, [refresh]);
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    /** Apply explicit appearance or react to OS appearance changes. */
    const applyTheme = () => {
      document.documentElement.dataset.theme =
        theme === "dark" || (theme === "system" && media.matches)
          ? "dark"
          : "light";
    };
    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [theme]);
  useEffect(() => {
    if (!isDesktop) return;
    let canceled = false;
    let unlisten: (() => void) | undefined;
    /** Queue delivered URLs once across cold-start and running-instance events. */
    function receive(urls: string[] | null) {
      if (canceled) return;
      const now = Date.now();
      for (const [url, time] of seenLinks.current)
        if (now - time > 10000) seenLinks.current.delete(url);
      const accepted = (urls ?? []).filter((url) => {
        if (seenLinks.current.has(url)) return false;
        seenLinks.current.set(url, now);
        return true;
      });
      if (accepted.length)
        setLinks((old) => [...old, ...accepted].slice(0, 10));
    }
    void import("@tauri-apps/plugin-deep-link")
      .then(async (plugin) => {
        const off = await plugin.onOpenUrl(receive);
        if (canceled) {
          off();
          return;
        }
        unlisten = off;
        receive(await plugin.getCurrent());
      })
      .catch(() => {
        if (!canceled)
          setNotice({
            kind: "error",
            text: "链接监听未就绪；仍可使用“导入链接”粘贴导入。",
          });
      });
    return () => {
      canceled = true;
      unlisten?.();
    };
  }, []);
  useEffect(() => {
    if (!links.length || modal || busy || !data) return;
    const link = links[0];
    setLinks((old) => old.slice(1));
    setBusy(true);
    void api
      .importPreview(link)
      .then((preview) => setModal({ kind: "import-review", preview }))
      .catch((e) => setNotice({ kind: "error", text: String(e) }))
      .finally(() => setBusy(false));
  }, [links, modal, busy, data]);

  /** Run one UI mutation with visible progress and safe error reporting. */
  async function run(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setNotice(null);
    try {
      await action();
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(false);
    }
  }
  /** Discard staged native contents when the user cancels either kind of preview. */
  function closeModal() {
    if (busy) return;
    if (modal?.kind === "review" || modal?.kind === "import-review")
      void api.cancel(modal.preview.token).catch(() => {});
    setModal(null);
  }
  /** Refresh persisted data and display a concise successful outcome. */
  async function done(text: string) {
    setModal(null);
    await refresh();
    setNotice({ kind: "success", text });
  }
  /** Test one saved model on demand; bind its transient indicator to the exact saved configuration. */
  async function testLibraryModel(model: ModelConfig) {
    if (pendingTests.current.has(model.id)) return;
    const request = Symbol();
    pendingTests.current.set(model.id, request);
    const key = modelTestKey(model);
    setModelTests((old) => ({
      ...old,
      [model.id]: {
        key,
        status: "testing",
        message: "正在发送 test，最长等待 30 秒…",
      },
    }));
    try {
      const result = await api.testModel(structuredClone(model));
      if (pendingTests.current.get(model.id) !== request) return;
      setModelTests((old) =>
        old[model.id]?.key === key && old[model.id]?.status === "testing"
          ? {
              ...old,
              [model.id]: {
                key,
                status: "passed",
                message: modelTestSuccess(result),
                expiresAt: Date.now() + 5000,
              },
            }
          : old,
      );
    } catch (error) {
      if (pendingTests.current.get(model.id) !== request) return;
      setModelTests((old) =>
        old[model.id]?.key === key && old[model.id]?.status === "testing"
          ? {
              ...old,
              [model.id]: {
                key,
                status: "failed",
                message: String(error),
                expiresAt: Date.now() + 5000,
              },
            }
          : old,
      );
    } finally {
      if (pendingTests.current.get(model.id) === request)
        pendingTests.current.delete(model.id);
    }
  }
  const visible = (data?.models ?? []).filter(
    (m) =>
      (filter === "all" || nativeAgent[m.protocol] === filter) &&
      `${m.name} ${m.modelId} ${m.baseUrl}`
        .toLowerCase()
        .includes(query.toLowerCase()),
  );
  const titles = {
    models: ["模型库", "把合适的模型，交给合适的 Agent。"],
    backups: ["模型配置备份", "每一次切换，都留有回去的路。"],
    settings: ["设置", "让配置找到正确的位置。"],
    skills: ["技能", "全局安装，为每个 Agent 连接合适的技能。"],
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <a
          className="brand"
          href="#"
          onClick={(e) => {
            e.preventDefault();
            setPage("models");
          }}
        >
          <img src="/icon.png" alt="power-switch 图标" />
          <span>
            power<span className="brand-dash">-</span>switch
            <small>YOUR MODELS. YOUR CHOICE.</small>
          </span>
        </a>
        <nav aria-label="主导航">
          <button
            className={page === "models" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("models")}
          >
            <Layers3 size={19} />
            模型库<span className="nav-count">{data?.models.length ?? 0}</span>
          </button>
          <button
            className={page === "backups" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("backups")}
          >
            <FileClock size={19} />
            模型配置备份
          </button>
          <button className="nav-item" disabled title="待开放">
            <BookOpen size={19} />
            技能<span className="nav-pending">待开放</span>
          </button>
          <button
            className={page === "settings" ? "nav-item active" : "nav-item"}
            onClick={() => setPage("settings")}
          >
            <Settings2 size={19} />
            设置
          </button>
        </nav>
        <div className="sidebar-bottom">
          <div className="privacy-box">
            <span className="privacy-icon">
              <ShieldCheck size={20} />
            </span>
            <strong>只在你的设备上</strong>
            <p>
              模型与密钥保存在本机
              <br />
              每次写入，自动备份
            </p>
          </div>
          <div className="version">
            <span className="status-dot" />
            power-switch <span>v0.1.1</span>
          </div>
        </div>
      </aside>
      <main className="main">
        <div className="topbar">
          <span>{titles[page][0]}</span>
          <div className="topbar-actions">
            {!isDesktop && (
              <span className="environment demo">
                <span className="status-dot" />
                浏览器演示 · 不写入文件
              </span>
            )}
            <button
              className="topbar-guide"
              type="button"
              onClick={() => setModal({ kind: "guide" })}
            >
              <CircleHelp size={15} /> 新手指引
            </button>
          </div>
        </div>
        <div className="page-content">
          <header className="page-header">
            <div>
              <h1>
                {titles[page][0]}
                <span className="heading-dot">.</span>
              </h1>
              <p>{titles[page][1]}</p>
            </div>
            {page === "models" && (
              <div className="header-actions">
                <button
                  className="button secondary"
                  disabled={busy}
                  onClick={() => setModal({ kind: "import" })}
                >
                  <Link2 size={16} />
                  导入链接
                </button>
                <button
                  className="button new-api-action"
                  disabled={busy}
                  onClick={() => setModal({ kind: "new-api" })}
                >
                  <Download size={16} />从 New API 添加
                </button>
                <button
                  className="button primary"
                  disabled={busy}
                  onClick={() => setModal({ kind: "model", model: newModel() })}
                >
                  <Plus size={18} />
                  添加模型
                </button>
              </div>
            )}
          </header>
          {notice && (
            <div
              role={notice.kind === "error" ? "alert" : "status"}
              className={`notification ${notice.kind}`}
            >
              {notice.kind === "success" ? (
                <CheckCircle2 size={19} />
              ) : (
                <CircleHelp size={19} />
              )}
              <span>{notice.text}</span>
              <button
                className="icon-button"
                aria-label="关闭提示"
                onClick={() => setNotice(null)}
              >
                <X size={16} />
              </button>
            </div>
          )}
          {!data ? (
            <div className="empty-state">
              <LoaderCircle className="spin" />
              <h2>正在打开模型库</h2>
              <button
                className="button secondary"
                onClick={() => void run(refresh)}
              >
                重新加载
              </button>
            </div>
          ) : (
            <>
              {page === "models" && (
                <>
                  <section className="intro-strip">
                    <div className="intro-copy">
                      <span className="intro-symbol">
                        <SlidersHorizontal size={23} />
                      </span>
                      <div>
                        <strong>一次整理，自由切换</strong>
                        <p>统一管理连接配置，为你的 Agent 选择下一位搭档。</p>
                      </div>
                    </div>
                    <div className="agent-chips">
                      <span>
                        <span className="mini-dot blue" />
                        WorkBuddy
                      </span>
                      <span>
                        <span className="mini-dot orange" />
                        Claude Code
                      </span>
                      <span>
                        <span className="mini-dot graphite" />
                        Codex
                      </span>
                    </div>
                  </section>
                  <div className="collection-tools">
                    <div className="filter-tabs" aria-label="按 Agent 筛选">
                      {[
                        ["all", "全部模型"],
                        ["workbuddy", "WorkBuddy"],
                        ["claude", "Claude Code"],
                        ["codex", "Codex"],
                      ].map(([value, label]) => (
                        <button
                          key={value}
                          className={filter === value ? "selected" : ""}
                          onClick={() => setFilter(value)}
                        >
                          {label}
                          {value === "all" && <span>{data.models.length}</span>}
                        </button>
                      ))}
                    </div>
                    <label className="search">
                      <Search size={16} />
                      <input
                        aria-label="搜索模型"
                        placeholder="搜索名称、ID 或地址"
                        value={query}
                        onChange={(e) => setQuery(e.target.value)}
                      />
                      {query && (
                        <button
                          className="icon-button"
                          aria-label="清空搜索"
                          onClick={() => setQuery("")}
                        >
                          <X size={14} />
                        </button>
                      )}
                    </label>
                  </div>
                  {visible.length ? (
                    <div className="model-list">
                      {visible.map((model, index) => {
                        const probe =
                          modelTests[model.id]?.key === modelTestKey(model)
                            ? modelTests[model.id]
                            : undefined;
                        return (
                          <article
                            className="model-card"
                            key={model.id}
                            style={{ animationDelay: `${index * 45}ms` }}
                          >
                            <div className="card-main">
                              <ProtocolMark
                                protocol={model.protocol}
                                name={model.name}
                              />
                              <div className="model-title">
                                <h2>
                                  {model.name}
                                  {(probe?.status === "passed" ||
                                    probe?.status === "failed") && (
                                    <span
                                      className={
                                        "model-test-dot " + probe.status
                                      }
                                      role="img"
                                      aria-label={
                                        probe.status === "passed"
                                          ? "模型测试通过"
                                          : "模型测试未通过"
                                      }
                                      title={probe.message}
                                    />
                                  )}
                                </h2>
                                <span className="model-id">
                                  {model.modelId}
                                </span>
                              </div>
                              <span
                                className={`tag protocol-tag ${model.protocol}`}
                              >
                                {protocolLabels[model.protocol]}
                              </span>
                            </div>
                            <div className="card-details">
                              <div>
                                <span className="detail-label">API 地址</span>
                                <span
                                  className="endpoint"
                                  title={model.baseUrl}
                                >
                                  {model.baseUrl}
                                  <ExternalLink size={12} />
                                </span>
                              </div>
                              <div className="credential">
                                <span className="detail-label">API KEY</span>
                                <span
                                  className={!model.apiKey ? "missing-key" : ""}
                                >
                                  {model.apiKey ? "••••••••••••" : "未设置"}
                                </span>
                              </div>
                              <div>
                                <span className="detail-label">适用 AGENT</span>
                                <span>
                                  {agentLabels[nativeAgent[model.protocol]]}
                                </span>
                              </div>
                            </div>
                            <div className="card-footer">
                              <div className="capability-tags">
                                {model.supportsToolCall && (
                                  <span>工具调用</span>
                                )}
                                {model.supportsImages && <span>图像输入</span>}
                                {model.contextWindow && (
                                  <span>
                                    {Math.round(model.contextWindow / 1000)}K
                                    上下文
                                  </span>
                                )}
                                {!!model.reasoningLevels.length && (
                                  <span>推理模型</span>
                                )}
                              </div>
                              <div className="card-actions">
                                <button
                                  className="icon-button"
                                  aria-label={`编辑 ${model.name}`}
                                  title="编辑"
                                  disabled={busy}
                                  onClick={() =>
                                    setModal({
                                      kind: "model",
                                      model: structuredClone(model),
                                    })
                                  }
                                >
                                  <Pencil size={15} />
                                </button>
                                <button
                                  className="icon-button"
                                  aria-label={`复制 ${model.name}`}
                                  title="复制模型"
                                  disabled={busy}
                                  onClick={() =>
                                    setModal({
                                      kind: "model",
                                      model: {
                                        ...structuredClone(model),
                                        id: "",
                                        name: `${model.name} · 副本`,
                                      },
                                    })
                                  }
                                >
                                  <Copy size={15} />
                                </button>
                                <button
                                  className="icon-button"
                                  aria-label={"测试模型 " + model.name}
                                  title="测试模型"
                                  disabled={busy || probe?.status === "testing"}
                                  onClick={() => void testLibraryModel(model)}
                                >
                                  {probe?.status === "testing" ? (
                                    <LoaderCircle size={15} className="spin" />
                                  ) : (
                                    <FlaskConical size={15} />
                                  )}
                                </button>
                                <button
                                  className="icon-button"
                                  aria-label={`分享 ${model.name}`}
                                  title="分享链接"
                                  disabled={busy}
                                  onClick={() =>
                                    setModal({ kind: "share", model })
                                  }
                                >
                                  <Link2 size={15} />
                                </button>
                                <button
                                  className="icon-button delete-button"
                                  aria-label={`删除 ${model.name}`}
                                  title="删除"
                                  disabled={busy}
                                  onClick={() =>
                                    setModal({ kind: "delete", model })
                                  }
                                >
                                  <Trash2 size={15} />
                                </button>
                                <span className="action-divider" />
                                <button
                                  className="apply-button"
                                  disabled={busy}
                                  onClick={() =>
                                    setModal({ kind: "agents", model })
                                  }
                                >
                                  应用到 Agent
                                  <ArrowRight size={15} />
                                </button>
                              </div>
                            </div>
                            {probe && (
                              <ModelTestFeedback
                                probe={probe}
                                className="card-test-feedback"
                                onDismiss={() =>
                                  setModelTests((old) =>
                                    old[model.id] === probe
                                      ? {
                                          ...old,
                                          [model.id]: {
                                            ...probe,
                                            dismissed: true,
                                          },
                                        }
                                      : old,
                                  )
                                }
                              />
                            )}
                          </article>
                        );
                      })}
                    </div>
                  ) : (
                    <div className="empty-state">
                      <div className="empty-icon">
                        <Layers3 size={30} />
                      </div>
                      <h2>
                        {query || filter !== "all"
                          ? "没有找到匹配的模型"
                          : "从你的第一个模型开始"}
                      </h2>
                      <p>
                        {query || filter !== "all"
                          ? "试试其他关键词或筛选条件。"
                          : "添加 API 地址和模型 ID，让不同 Agent 连接你的模型。"}
                      </p>
                      {!data.models.length && (
                        <button
                          className="button primary"
                          onClick={() =>
                            setModal({ kind: "model", model: newModel() })
                          }
                        >
                          <Plus size={17} />
                          添加第一个模型
                        </button>
                      )}
                    </div>
                  )}
                  <div className="collection-footnote">
                    <ShieldCheck size={15} />
                    <span>
                      保存模型不会修改 Agent
                      配置。应用前，你可以先预览全部变更。
                    </span>
                    <span className="footnote-count">
                      {visible.length} 个模型
                    </span>
                  </div>
                </>
              )}
              {page === "backups" && (
                <>
                  <div className="info-note">
                    <FileClock size={19} />
                    <span>
                      每次确认写入前，自动保存原始文件。恢复前也会创建一份新备份。
                    </span>
                  </div>
                  {data.backups.length ? (
                    <div className="backup-list">
                      {data.backups.map((backup) => (
                        <article className="backup-row" key={backup.id}>
                          <span className="backup-icon">
                            <FileClock size={22} />
                          </span>
                          <div>
                            <h3>{backup.title}</h3>
                            <p>
                              {formatTime(backup.createdAt)} ·{" "}
                              {backup.paths.length} 个文件
                            </p>
                            <details>
                              <summary>查看文件路径</summary>
                              {backup.paths.map((path) => (
                                <code key={path}>{path}</code>
                              ))}
                            </details>
                          </div>
                          <span
                            className={`tag ${backup.status === "completed" ? "green" : "amber"}`}
                          >
                            {{
                              completed: "已完成",
                              pending: "写入中断 · 可恢复",
                              rolled_back: "失败后已回滚",
                              rollback_failed: "需要恢复",
                            }[backup.status] ?? backup.status}
                          </span>
                          <button
                            className="button secondary"
                            disabled={busy}
                            onClick={() =>
                              void run(async () =>
                                setModal({
                                  kind: "review",
                                  preview: await api.restore(backup.id),
                                }),
                              )
                            }
                          >
                            预览恢复
                            <ArrowRight size={15} />
                          </button>
                          <button
                            className="icon-button delete-button"
                            disabled={busy}
                            aria-label={`删除备份 ${backup.title}`}
                            onClick={() =>
                              setModal({ kind: "delete-backup", backup })
                            }
                          >
                            <Trash2 size={16} />
                          </button>
                        </article>
                      ))}
                    </div>
                  ) : (
                    <div className="empty-state">
                      <div className="empty-icon">
                        <FileClock size={30} />
                      </div>
                      <h2>还没有模型配置备份</h2>
                      <p>第一次应用模型后，备份就会出现在这里。</p>
                    </div>
                  )}
                </>
              )}
              {page === "skills" && <SkillsPage />}
              {page === "settings" && (
                <SettingsPage
                  data={data}
                  busy={busy}
                  onThemePreview={setThemePreview}
                  onSave={(settings) =>
                    void run(async () => {
                      await api.settings(settings);
                      await refresh();
                      setNotice({ kind: "success", text: "设置已保存。" });
                    })
                  }
                />
              )}
            </>
          )}
          <footer className="page-bottom">
            <span>小小开关，更多可能。</span>
            <span>
              POWER YOUR WORKFLOW <Sparkles size={12} />
            </span>
          </footer>
        </div>
      </main>
      {modal?.kind === "guide" && (
        <NewcomerGuide
          onClose={closeModal}
          onStart={() => {
            setPage("models");
            setModal({ kind: "new-api" });
          }}
        />
      )}
      {modal?.kind === "new-api" && (
        <NewApiDialog onClose={closeModal} onAdded={refresh} />
      )}
      {modal?.kind === "model" && (
        <Modal
          title={modal.model.id ? "编辑模型" : "添加模型"}
          description="把服务商的连接信息保存在你的本地模型库。"
          onClose={closeModal}
          busy={busy}
        >
          <ModelForm
            initial={modal.model}
            busy={busy}
            onCancel={closeModal}
            onSave={(model, result) =>
              void run(async () => {
                const saved = await api.save(model);
                pendingTests.current.delete(saved.id);
                setModelTests((old) => ({
                  ...old,
                  [saved.id]: {
                    key: modelTestKey(saved),
                    status: "passed",
                    message: modelTestSuccess(result),
                    expiresAt: Date.now() + 5000,
                  },
                }));
                await done(
                  isDesktop
                    ? "模型已保存，尚未应用到 Agent。"
                    : "演示模型已保存至内存，刷新页面后重置。",
                );
              })
            }
          />
        </Modal>
      )}
      {modal?.kind === "agents" && (
        <Modal
          title="应用到 Agent"
          description="选择使用这个模型的软件。只显示原生协议兼容的选项。"
          onClose={closeModal}
          busy={busy}
        >
          <AgentPicker
            model={modal.model}
            busy={busy}
            onPreview={(agents, selectWorkbuddyModel) =>
              void run(async () =>
                setModal({
                  kind: "review",
                  preview: await api.preview(
                    modal.model.id,
                    agents,
                    selectWorkbuddyModel,
                  ),
                }),
              )
            }
          />
        </Modal>
      )}
      {modal?.kind === "review" && (
        <Modal
          title={modal.preview.title}
          description="请在确认覆盖前核对脱敏后的配置变更。"
          onClose={closeModal}
          wide
          busy={busy}
        >
          <ApplyReview
            preview={modal.preview}
            busy={busy}
            onCancel={closeModal}
            onConfirm={() =>
              void run(async () => {
                const result = await api.apply(modal.preview.token);
                await done(
                  [result.message, result.workbuddySelection]
                    .filter(Boolean)
                    .join(" "),
                );
              })
            }
          />
        </Modal>
      )}
      {modal?.kind === "import" && (
        <Modal
          title="从链接导入"
          description="粘贴 power-switch:// 链接，先预览，再决定保存。"
          onClose={closeModal}
          busy={busy}
        >
          <ImportLinkForm
            busy={busy}
            onPreview={(link) =>
              void run(async () =>
                setModal({
                  kind: "import-review",
                  preview: await api.importPreview(link),
                }),
              )
            }
          />
        </Modal>
      )}
      {modal?.kind === "import-review" && (
        <Modal
          title="确认导入模型"
          description="只加入模型库，不会修改任何 Agent 配置。"
          onClose={closeModal}
          busy={busy}
        >
          <ImportReview
            preview={modal.preview}
            busy={busy}
            onConfirm={(updates) =>
              void run(async () => {
                const count = await api.importConfirm(
                  modal.preview.token,
                  updates,
                );
                await done(`已导入 ${count} 个模型，尚未应用到 Agent。`);
              })
            }
          />
        </Modal>
      )}
      {modal?.kind === "share" && (
        <Modal
          title="分享模型配置"
          description="生成可在浏览器中打开的 power-switch 模型链接。"
          onClose={closeModal}
        >
          <ShareModel model={modal.model} />
        </Modal>
      )}
      {modal?.kind === "delete-backup" && (
        <Modal
          title="删除模型配置备份"
          description={`二次确认：删除“${modal.backup.title}”的备份？`}
          onClose={closeModal}
          busy={busy}
        >
          <div className="warning-note">
            <Trash2 size={18} />
            <div>
              删除后无法使用这条记录恢复配置。当前 Agent 配置文件不受影响。
            </div>
          </div>
          <p className="field-hint">
            {formatTime(modal.backup.createdAt)} · {modal.backup.paths.length}{" "}
            个文件
          </p>
          <div className="modal-footer">
            <button
              className="button secondary"
              disabled={busy}
              onClick={closeModal}
            >
              取消
            </button>
            <button
              className="button danger"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await api.deleteBackup(modal.backup.id);
                  await done("模型配置备份已删除，Agent 配置保持不变。");
                })
              }
            >
              确认删除备份
            </button>
          </div>
        </Modal>
      )}
      {modal?.kind === "delete" && (
        <Modal
          title="删除模型"
          description={`将“${modal.model.name}”从 power-switch 模型库中移除。`}
          onClose={closeModal}
          busy={busy}
        >
          <div className="info-note">
            <CircleHelp size={18} />
            <span>已应用的 Agent 配置保留，可通过模型配置备份恢复。</span>
          </div>
          <div className="modal-footer">
            <button
              className="button secondary"
              disabled={busy}
              onClick={closeModal}
            >
              取消
            </button>
            <button
              className="button danger"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await api.delete(modal.model.id);
                  await done("模型已从模型库删除。");
                })
              }
            >
              确认删除
              <Trash2 size={16} />
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}

/** Accept a pasted deep link without putting its potentially sensitive value in logs. */
function ImportLinkForm({
  busy,
  onPreview,
}: {
  busy: boolean;
  onPreview: (link: string) => void;
}) {
  const [link, setLink] = useState("");
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onPreview(link);
      }}
    >
      <label className="field">
        模型链接
        <textarea
          autoFocus
          value={link}
          onChange={(e) => setLink(e.target.value)}
          placeholder="power-switch://model/import?v=1&data=…"
          rows={5}
          spellCheck={false}
          required
          maxLength={65536}
        />
      </label>
      <div className="info-note">
        <Link2 size={18} />
        <span>支持单个或批量模型。链接中的 API Key 在预览中保持隐藏。</span>
      </div>
      <div className="modal-footer">
        <button className="button primary" disabled={busy || !link.trim()}>
          {busy ? "正在解析…" : "预览导入"}
          <ArrowRight size={16} />
        </button>
      </div>
    </form>
  );
}

/** Let duplicate imports opt in to replacement while new rows import by default. */
function ImportReview({
  preview,
  busy,
  onConfirm,
}: {
  preview: ImportPreview;
  busy: boolean;
  onConfirm: (indexes: number[]) => void;
}) {
  const [updates, setUpdates] = useState<number[]>([]);
  return (
    <>
      <div className="import-rows">
        {preview.rows.map((row) => (
          <div className="import-row" key={row.index}>
            <ProtocolMark protocol={row.protocol} name={row.name} small />
            <div>
              <strong>{row.name}</strong>
              <p>{row.modelId}</p>
              <small>{row.baseUrl}</small>
              <span className="field-hint">
                {row.hasApiKey ? "已包含密钥（隐藏）" : "未包含密钥"}
              </span>
            </div>
            {row.duplicate ? (
              <label className="duplicate-choice">
                <input
                  type="checkbox"
                  checked={updates.includes(row.index)}
                  onChange={(e) =>
                    setUpdates(
                      e.target.checked
                        ? [...updates, row.index]
                        : updates.filter((i) => i !== row.index),
                    )
                  }
                />
                更新重复项
              </label>
            ) : (
              <span className="tag green">新增</span>
            )}
          </div>
        ))}
      </div>
      <div className="modal-footer">
        <button
          className="button primary"
          disabled={busy}
          onClick={() => onConfirm(updates)}
        >
          确认导入
          <Download size={16} />
        </button>
      </div>
    </>
  );
}

/** Generate credential-free links by default and guard asynchronous key-inclusion toggles. */
function ShareModel({ model }: { model: ModelConfig }) {
  const [include, setInclude] = useState(false);
  const [link, setLink] = useState("");
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    let alive = true;
    setLink("");
    setCopied(false);
    setError("");
    void api
      .share(model.id, include)
      .then((value) => {
        if (alive) setLink(value);
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
    };
  }, [model.id, include]);
  /** Copy only the current completed link after the user explicitly presses the button. */
  async function copy() {
    try {
      await navigator.clipboard.writeText(link);
      setCopied(true);
    } catch {
      setError("复制失败，请选中下方链接手动复制。");
    }
  }
  return (
    <>
      <label className="checkbox-line">
        <input
          type="checkbox"
          checked={include}
          onChange={(e) => setInclude(e.target.checked)}
        />
        包含 API Key（接收者将获得该密钥）
      </label>
      <textarea
        aria-label="分享链接"
        value={link}
        readOnly
        rows={5}
        spellCheck={false}
      />
      {error && (
        <p role="alert" className="inline-error">
          {error}
        </p>
      )}
      <div className="modal-footer">
        <button
          className="button primary"
          disabled={!link}
          onClick={() => void copy()}
        >
          {copied ? <Check size={16} /> : <Copy size={16} />}{" "}
          {copied ? "已复制" : "复制链接"}
        </button>
      </div>
    </>
  );
}

/** Edit path overrides while displaying the OS-resolved paths and local storage location. */
function SettingsPage({
  data,
  busy,
  onThemePreview,
  onSave,
}: {
  data: AppData;
  busy: boolean;
  onThemePreview: (theme: Settings["theme"] | null) => void;
  onSave: (settings: Settings) => void;
}) {
  const [settings, setSettings] = useState(data.settings);
  useEffect(() => {
    // Preview the draft immediately; leaving settings restores the saved theme.
    onThemePreview(settings.theme);
    return () => onThemePreview(null);
  }, [settings.theme, onThemePreview]);
  const [pickerError, setPickerError] = useState("");
  const [picking, setPicking] = useState(false);
  /** Update the editable path only after a native selection; saving remains explicit. */
  async function choosePath(
    key: "workbuddyPath" | "claudePath" | "codexDir",
    kind: "file" | "directory",
    label: string,
  ) {
    setPicking(true);
    setPickerError("");
    try {
      const chosen = await pickLocalPath(
        kind,
        `选择 ${label} 配置${kind === "file" ? "文件" : "目录"}`,
        settings[key] || undefined,
      );
      if (chosen) {
        const file = key === "workbuddyPath" ? "models.json" : "settings.json";
        const separator = chosen.includes("\\") ? "\\" : "/";
        const value =
          kind === "directory" && key !== "codexDir"
            ? chosen.replace(/[\\/]+$/, "") + separator + file
            : chosen;
        setSettings((old) => ({ ...old, [key]: value }));
      }
    } catch (error) {
      setPickerError(String(error));
    } finally {
      setPicking(false);
    }
  }
  /** Save all settings together so path validation is atomic on the backend. */
  function submit(event: FormEvent) {
    event.preventDefault();
    onSave(settings);
  }
  return (
    <form className="settings-form" onSubmit={submit}>
      <section className="settings-section">
        <div className="section-title">
          <Monitor size={19} />
          <div>
            <h2>外观</h2>
            <p>选择后立即预览，保存设置后保留。</p>
          </div>
        </div>
        <div className="theme-options">
          {(
            [
              { value: "light", label: "浅色", Icon: Sun },
              { value: "dark", label: "深色", Icon: Moon },
              { value: "system", label: "跟随系统", Icon: Monitor },
            ] as const
          ).map(({ value, label, Icon }) => (
            <button
              type="button"
              key={value}
              className={settings.theme === value ? "selected" : ""}
              aria-pressed={settings.theme === value}
              disabled={busy}
              onClick={() => setSettings({ ...settings, theme: value })}
            >
              <Icon size={22} />
              {label}
              {settings.theme === value && <Check size={15} />}
            </button>
          ))}
        </div>
      </section>
      <section className="settings-section">
        <div className="section-title">
          <FolderCog size={19} />
          <div>
            <h2>Agent 配置位置</h2>
            <p>自动识别当前用户目录，也可选择本地文件或目录，保存后生效。</p>
          </div>
        </div>
        {data.agents.map((agent) => {
          const key =
            agent.agent === "workbuddy"
              ? "workbuddyPath"
              : agent.agent === "claude"
                ? "claudePath"
                : "codexDir";
          return (
            <div className="field path-field" key={agent.agent}>
              <span>
                {agentLabels[agent.agent]}{" "}
                {agent.agent === "codex" ? "配置目录" : "配置文件"}
                <span className={`tag ${agent.exists ? "green" : ""}`}>
                  {agent.exists ? "已找到配置" : "尚未创建"}
                </span>
              </span>
              <div className="path-picker-row">
                <input
                  aria-label={`${agentLabels[agent.agent]} 配置路径`}
                  value={settings[key] ?? ""}
                  placeholder="自动识别（推荐）"
                  onChange={(e) =>
                    setSettings({ ...settings, [key]: e.target.value || null })
                  }
                  spellCheck={false}
                />
                {agent.agent !== "codex" && (
                  <button
                    type="button"
                    className="button secondary"
                    disabled={busy || picking}
                    onClick={() =>
                      void choosePath(key, "file", agentLabels[agent.agent])
                    }
                    aria-label={`选择 ${agentLabels[agent.agent]} 配置文件`}
                  >
                    <FolderOpen size={15} />
                    选择文件
                  </button>
                )}
                <button
                  type="button"
                  className="button secondary"
                  disabled={busy || picking}
                  onClick={() =>
                    void choosePath(key, "directory", agentLabels[agent.agent])
                  }
                  aria-label={`选择 ${agentLabels[agent.agent]} 配置目录`}
                >
                  <FolderOpen size={15} />
                  选择目录
                </button>
              </div>
              <span className="resolved-path">当前路径：{agent.path}</span>
            </div>
          );
        })}
      </section>
      {pickerError && (
        <p role="alert" className="inline-error">
          {pickerError}
        </p>
      )}
      <section className="settings-section storage-section">
        <div className="section-title">
          <ShieldCheck size={19} />
          <div>
            <h2>本地数据</h2>
            <p>模型与 API Key 保存在本机 JSON 文件中，备份包含原始配置。</p>
          </div>
        </div>
        <code>{data.dataDir}</code>
      </section>
      <div className="settings-actions">
        <span>
          <ArrowDownToLine size={15} />
          保存设置不会应用模型
        </span>
        <button className="button primary" disabled={busy}>
          保存设置
          <Check size={16} />
        </button>
      </div>
    </form>
  );
}
