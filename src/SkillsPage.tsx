import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type FormEvent,
} from "react";
import {
  BookOpen,
  Check,
  FolderOpen,
  GitBranch,
  Globe2,
  Link2,
  LoaderCircle,
  RefreshCw,
  Search,
  Trash2,
  Upload,
} from "lucide-react";
import { Modal } from "./components";
import { pickLocalPath } from "./file-picker";
import {
  scopeLabels,
  skillsApi,
  type SkillScope,
  type SkillGroup,
  type SkillEntry,
  type InstallPreview,
  type SkillChangePreview,
  type InstallSource,
} from "./skills-api";
import "./skills.css";

type Dialog =
  | { kind: "install" }
  | { kind: "install-review"; preview: InstallPreview }
  | { kind: "link"; skill: SkillEntry }
  | { kind: "change"; preview: SkillChangePreview; deleting: boolean }
  | null;
const scopes: SkillScope[] = ["global", "claude", "codex", "workbuddy"];

/** Browse skill inventories with a resettable one-minute refresh and explicit confirmations. */
export function SkillsPage() {
  const [groups, setGroups] = useState<SkillGroup[]>([]);
  const [scope, setScope] = useState<SkillScope>("global");
  const [selected, setSelected] = useState("");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [scanError, setScanError] = useState("");
  const [message, setMessage] = useState("");
  const [document, setDocument] = useState("");
  const [detailError, setDetailError] = useState("");
  const [detailLoading, setDetailLoading] = useState(false);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [countdown, setCountdown] = useState(60);
  const [revision, setRevision] = useState(0);
  const mounted = useRef(false);
  const scanning = useRef(false);
  const nextRefresh = useRef(Date.now() + 60000);
  const token = useRef<string | null>(null);

  /** Start the next minute at the time refresh is requested and avoid overlapping scans. */
  const refresh = useCallback(async () => {
    nextRefresh.current = Date.now() + 60000;
    setCountdown(60);
    if (scanning.current) return;
    scanning.current = true;
    setLoading(true);
    try {
      const rows = await skillsApi.list();
      if (mounted.current) {
        setGroups(rows);
        setScanError("");
        setRevision((v) => v + 1);
      }
    } catch (e) {
      if (mounted.current) setScanError(String(e));
    } finally {
      scanning.current = false;
      if (mounted.current) setLoading(false);
    }
  }, []);
  useEffect(() => {
    mounted.current = true;
    void refresh();
    const timer = window.setInterval(() => {
      const remaining = Math.max(
        0,
        Math.ceil((nextRefresh.current - Date.now()) / 1000),
      );
      setCountdown(remaining);
      if (remaining === 0 && !scanning.current) void refresh();
    }, 1000);
    return () => {
      mounted.current = false;
      window.clearInterval(timer);
      if (token.current) void skillsApi.cancel(token.current).catch(() => {});
    };
  }, [refresh]);

  const group = groups.find((g) => g.scope === scope);
  const visible = (group?.skills ?? []).filter((s) =>
    (s.name + " " + s.id + " " + s.description)
      .toLowerCase()
      .includes(query.toLowerCase()),
  );
  const skill = visible.find((s) => s.id === selected) ?? visible[0];
  useEffect(() => {
    let alive = true;
    setDocument("");
    setDetailError("");
    if (!skill) {
      setDetailLoading(false);
      return;
    }
    setDetailLoading(true);
    void skillsApi
      .detail(scope, skill.id)
      .then((result) => {
        if (alive) setDocument(result.document);
      })
      .catch((e) => {
        if (alive) setDetailError(String(e));
      })
      .finally(() => {
        if (alive) setDetailLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [scope, skill?.id, revision]);

  /** Serialize user-triggered actions while showing errors inside the active dialog. */
  async function run(action: () => Promise<void>) {
    if (busy) return;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await action();
    } catch (e) {
      if (mounted.current) setError(String(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  /** Discard delayed previews if the user navigated away while a source was downloading. */
  function showPreview(
    next:
      | { kind: "install-review"; preview: InstallPreview }
      | { kind: "change"; preview: SkillChangePreview; deleting: boolean },
  ) {
    if (!mounted.current) {
      void skillsApi.cancel(next.preview.token).catch(() => {});
      return;
    }
    token.current = next.preview.token;
    setDialog(next);
  }
  /** Cancel staging without modifying any installed skill. */
  function close() {
    if (busy) return;
    if (token.current) void skillsApi.cancel(token.current).catch(() => {});
    token.current = null;
    setDialog(null);
    setError("");
  }
  /** Commit a reviewed token once and refresh the authoritative inventory afterward. */
  async function confirm(indexes: number[] = []) {
    if (!token.current) return;
    const current = token.current;
    token.current = null;
    try {
      const result = await skillsApi.commit(current, indexes);
      if (mounted.current) {
        setDialog(null);
        setMessage(result);
        await refresh();
      }
    } catch (error) {
      // A native commit consumes its token even when a stale preview is rejected.
      void skillsApi.cancel(current).catch(() => {});
      if (mounted.current) {
        setDialog(null);
        await refresh();
      }
      throw error;
    }
  }

  return (
    <section className="skills-page" aria-label="技能管理">
      <div className="skills-toolbar">
        <div className="skill-scope-tabs" role="tablist" aria-label="技能位置">
          {scopes.map((value) => (
            <button
              key={value}
              role="tab"
              aria-selected={scope === value}
              className={scope === value ? "selected" : ""}
              onClick={() => {
                setScope(value);
                setSelected("");
                setQuery("");
              }}
            >
              {value === "global" && <Globe2 size={14} />} {scopeLabels[value]}{" "}
              <span>
                {groups.find((g) => g.scope === value)?.skills.length ?? 0}
              </span>
            </button>
          ))}
        </div>
        <div className="skills-refresh">
          <span aria-live="off">
            {loading ? "正在扫描…" : countdown + " 秒后自动刷新"}
          </span>
          <button
            className="button secondary"
            disabled={loading}
            onClick={() => void refresh()}
          >
            <RefreshCw size={15} className={loading ? "spin" : ""} />
            刷新
          </button>
        </div>
      </div>
      <div className="skills-location">
        <div>
          <FolderOpen size={17} />
          <code>{group?.path ?? "正在获取技能目录…"}</code>
        </div>
        {scope === "global" ? (
          <button
            className="button primary"
            disabled={busy}
            onClick={() => {
              setError("");
              setDialog({ kind: "install" });
            }}
          >
            <Upload size={16} />
            安装技能
          </button>
        ) : (
          <button
            className="button secondary"
            onClick={() => {
              setScope("global");
              setSelected("");
              setQuery("");
            }}
          >
            前往全局安装
          </button>
        )}
      </div>
      {message && (
        <p className="skills-message" role="status">
          <Check size={17} />
          {message}
        </p>
      )}
      {!dialog && error && (
        <p className="inline-error" role="alert">
          {error}
        </p>
      )}
      {scanError && (
        <p className="inline-error" role="alert">
          {scanError}
        </p>
      )}
      {group?.error && (
        <p className="inline-error" role="alert">
          {group.error}
        </p>
      )}
      <div className="skills-workspace">
        <div className="skill-index">
          <label className="search">
            <Search size={16} />
            <input
              aria-label="搜索技能"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="搜索名称或描述"
            />
          </label>
          <div className="skill-index-heading">
            {scopeLabels[scope]}技能 <span>{visible.length}</span>
          </div>
          <div className="skill-options" role="listbox" aria-label="技能清单">
            {visible.map((row) => (
              <button
                className={
                  "skill-option " + (row.id === skill?.id ? "selected" : "")
                }
                key={row.id}
                role="option"
                aria-selected={row.id === skill?.id}
                onClick={() => setSelected(row.id)}
              >
                <span className="skill-option-title">
                  <BookOpen size={16} />
                  <strong>{row.name}</strong>
                  {row.linked && <Link2 size={13} aria-label="软链接" />}
                </span>
                <span className="skill-option-description">
                  {row.problem ?? (row.description || row.id)}
                </span>
              </button>
            ))}
            {!visible.length && (
              <div className="skills-empty">
                <BookOpen size={27} />
                <p>
                  {loading
                    ? "正在读取技能…"
                    : query
                      ? "没有匹配的技能"
                      : "此目录还没有技能"}
                </p>
              </div>
            )}
          </div>
        </div>
        <article className="skill-detail" aria-label="技能详情">
          {skill ? (
            <>
              <header className="skill-detail-header">
                <div>
                  <span className="eyebrow">SKILL DETAIL</span>
                  <h2>{skill.name}</h2>
                </div>
                <span className={"tag " + (skill.linked ? "green" : "")}>
                  {skill.linked ? "软链接" : "本地技能"}
                </span>
              </header>
              <p className="skill-description">
                {skill.description || "此技能尚未提供描述。"}
              </p>
              <dl className="skill-paths">
                <dt>目录</dt>
                <dd>{skill.path}</dd>
                {skill.linkTarget && (
                  <>
                    <dt>链接目标</dt>
                    <dd>{skill.linkTarget}</dd>
                  </>
                )}
              </dl>
              <div className="skill-detail-actions">
                {scope === "global" && (
                  <button
                    className="button secondary"
                    disabled={busy || !!skill.problem}
                    onClick={() => {
                      setError("");
                      setDialog({ kind: "link", skill });
                    }}
                  >
                    <Link2 size={16} />
                    连接到 Agent
                  </button>
                )}
                <button
                  className="button secondary skill-remove"
                  disabled={busy}
                  onClick={() =>
                    void run(async () =>
                      showPreview({
                        kind: "change",
                        deleting: true,
                        preview: await skillsApi.deletePreview(scope, skill.id),
                      }),
                    )
                  }
                >
                  <Trash2 size={15} />
                  删除技能
                </button>
              </div>
              <div className="skill-document-title">
                SKILL.md <span>只读预览</span>
              </div>
              {detailLoading ? (
                <div className="skills-empty">
                  <LoaderCircle className="spin" />
                </div>
              ) : detailError ? (
                <p className="inline-error" role="alert">
                  {detailError}
                </p>
              ) : (
                <pre className="skill-document">{document}</pre>
              )}
            </>
          ) : (
            <div className="skills-empty skill-detail-empty">
              <BookOpen size={37} />
              <h2>让技能各就其位</h2>
              <p>
                选择左侧技能查看详情。
                <br />
                安装到全局后，可用软链接连接到各 Agent。
              </p>
            </div>
          )}
        </article>
      </div>
      {dialog && (
        <Modal
          title={
            dialog.kind === "install"
              ? "安装到全局"
              : dialog.kind === "install-review"
                ? "确认安装技能"
                : dialog.kind === "link"
                  ? "连接到 Agent"
                  : dialog.preview.title
          }
          description={
            dialog.kind === "install"
              ? "选择本地技能目录、SKILL.md 文件，或公开 HTTPS Git 仓库。"
              : dialog.kind === "install-review"
                ? "技能只会安装到全局目录。请先核对名称和内容来源。"
                : dialog.kind === "link"
                  ? "技能保留在全局，Agent 通过软链接读取相同内容。"
                  : "二次确认：请核对本次操作会影响的目录。"
          }
          onClose={close}
          busy={busy}
          wide={dialog.kind === "install-review"}
        >
          {error && (
            <p role="alert" className="inline-error">
              {error}
            </p>
          )}
          {dialog.kind === "install" && (
            <InstallForm
              busy={busy}
              onPreview={(source) =>
                void run(async () =>
                  showPreview({
                    kind: "install-review",
                    preview: await skillsApi.installPreview(source),
                  }),
                )
              }
            />
          )}
          {dialog.kind === "install-review" && (
            <InstallReview
              preview={dialog.preview}
              busy={busy}
              onCancel={close}
              onConfirm={(indexes) => void run(() => confirm(indexes))}
            />
          )}
          {dialog.kind === "link" && (
            <LinkForm
              busy={busy}
              onPreview={(targets) =>
                void run(async () =>
                  showPreview({
                    kind: "change",
                    deleting: false,
                    preview: await skillsApi.linkPreview(
                      dialog.skill.id,
                      targets,
                    ),
                  }),
                )
              }
            />
          )}
          {dialog.kind === "change" && (
            <>
              <p
                className={
                  dialog.deleting ? "skills-delete-warning" : "info-note"
                }
              >
                {dialog.preview.message}
              </p>
              <div className="skill-confirm-paths">
                {dialog.preview.paths.map((path) => (
                  <code key={path}>{path}</code>
                ))}
              </div>
              {dialog.deleting && (
                <p className="field-hint">
                  将移入对应技能目录的 .power-switch-trash
                  回收目录，可手动恢复。删除软链接不会删除其实际目标。
                </p>
              )}
              <div className="modal-footer">
                <button
                  className="button secondary"
                  disabled={busy}
                  onClick={close}
                >
                  取消
                </button>
                <button
                  className={
                    "button " + (dialog.deleting ? "danger" : "primary")
                  }
                  disabled={busy}
                  onClick={() => void run(() => confirm())}
                >
                  {dialog.deleting ? "确认删除技能" : "确认建立软链接"}
                </button>
              </div>
            </>
          )}
        </Modal>
      )}
    </section>
  );
}

/** Collect only the source; the native backend fixes the installation destination to global. */
function InstallForm({
  busy,
  onPreview,
}: {
  busy: boolean;
  onPreview: (source: InstallSource) => void;
}) {
  const [kind, setKind] = useState<"local" | "git">("local");
  const [path, setPath] = useState("");
  const [url, setUrl] = useState("");
  const [subdir, setSubdir] = useState("");
  const [picking, setPicking] = useState(false);
  const [error, setError] = useState("");
  /** Preserve the source when the native file or directory picker is canceled. */
  async function browse(type: "file" | "directory") {
    setPicking(true);
    setError("");
    try {
      const result = await pickLocalPath(
        type,
        type === "file" ? "选择 SKILL.md" : "选择技能目录",
      );
      if (result) setPath(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setPicking(false);
    }
  }
  /** Stage the source before presenting the install confirmation. */
  function submit(event: FormEvent) {
    event.preventDefault();
    onPreview(
      kind === "local"
        ? { kind, path }
        : { kind, url, subdir: subdir.trim() || null },
    );
  }
  return (
    <form onSubmit={submit}>
      <div className="skill-source-tabs">
        <button
          type="button"
          className={"button " + (kind === "local" ? "primary" : "secondary")}
          disabled={busy}
          onClick={() => setKind("local")}
        >
          <FolderOpen size={16} />
          本地技能
        </button>
        <button
          type="button"
          className={"button " + (kind === "git" ? "primary" : "secondary")}
          disabled={busy}
          onClick={() => setKind("git")}
        >
          <GitBranch size={16} />
          Git 仓库
        </button>
      </div>
      {kind === "local" ? (
        <>
          <label className="field">
            技能目录或 SKILL.md
            <input
              autoFocus
              required
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="选择或粘贴完整的本地路径"
              disabled={busy}
            />
          </label>
          <div className="skill-source-tabs">
            <button
              type="button"
              className="button secondary"
              disabled={busy || picking}
              onClick={() => void browse("directory")}
            >
              选择目录
            </button>
            <button
              type="button"
              className="button secondary"
              disabled={busy || picking}
              onClick={() => void browse("file")}
            >
              选择 SKILL.md
            </button>
          </div>
        </>
      ) : (
        <>
          <label className="field">
            HTTPS Git 仓库地址
            <input
              required
              type="url"
              value={url}
              onChange={(e) => setUrl(e.target.value)}
              placeholder="https://github.com/owner/skills"
              disabled={busy}
            />
          </label>
          <label className="field">
            仓库子目录（可选）
            <input
              value={subdir}
              onChange={(e) => setSubdir(e.target.value)}
              placeholder="例如 skills/code-review；留空扫描仓库"
              disabled={busy}
            />
          </label>
          <p className="field-hint">
            支持 GitHub tree 链接。需要本机安装
            Git；不会执行仓库中的脚本或安装依赖。
          </p>
        </>
      )}
      {error && (
        <p className="inline-error" role="alert">
          {error}
        </p>
      )}
      <div className="modal-footer">
        <button className="button primary" disabled={busy || picking}>
          {busy ? "正在准备预览…" : "预览安装"}
        </button>
      </div>
    </form>
  );
}

/** Keep existing directories protected while selecting staged packages for installation. */
function InstallReview({
  preview,
  busy,
  onCancel,
  onConfirm,
}: {
  preview: InstallPreview;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (indexes: number[]) => void;
}) {
  const [selected, setSelected] = useState<number[]>(
    preview.skills.length === 1 && !preview.skills[0].exists
      ? [preview.skills[0].index]
      : [],
  );
  return (
    <>
      <p className="field-hint">目标：{preview.destination}</p>
      <div className="skill-install-candidates">
        {preview.skills.map((row) => (
          <label className="skill-install-candidate" key={row.index}>
            <input
              type="checkbox"
              disabled={busy || row.exists}
              checked={selected.includes(row.index)}
              onChange={(e) =>
                setSelected((old) =>
                  e.target.checked
                    ? [...old, row.index]
                    : old.filter((i) => i !== row.index),
                )
              }
            />
            <span>
              <strong>{row.name}</strong>
              <span>{row.description || row.folder}</span>
              <small>
                {row.exists ? "已存在同名目录，不会覆盖" : row.folder}
              </small>
            </span>
          </label>
        ))}
      </div>
      <div className="modal-footer">
        <span className="field-hint">已选 {selected.length} / 50</span>
        <button className="button secondary" disabled={busy} onClick={onCancel}>
          取消
        </button>
        <button
          className="button primary"
          disabled={busy || !selected.length || selected.length > 50}
          onClick={() => onConfirm(selected)}
        >
          确认安装到全局
        </button>
      </div>
    </>
  );
}

/** Choose only Agent scopes, then request authoritative link paths from the backend. */
function LinkForm({
  busy,
  onPreview,
}: {
  busy: boolean;
  onPreview: (scopes: SkillScope[]) => void;
}) {
  const [selected, setSelected] = useState<SkillScope[]>([]);
  return (
    <>
      <div className="agent-options">
        {scopes
          .filter((s) => s !== "global")
          .map((scope) => (
            <label className="checkbox-line" key={scope}>
              <input
                type="checkbox"
                disabled={busy}
                checked={selected.includes(scope)}
                onChange={(e) =>
                  setSelected((old) =>
                    e.target.checked
                      ? [...old, scope]
                      : old.filter((s) => s !== scope),
                  )
                }
              />
              {scopeLabels[scope]}
            </label>
          ))}
      </div>
      <p className="field-hint">
        已有同名技能或软链接会阻止此次操作，不会覆盖。
      </p>
      <div className="modal-footer">
        <button
          className="button primary"
          disabled={busy || !selected.length}
          onClick={() => onPreview(selected)}
        >
          预览软链接
        </button>
      </div>
    </>
  );
}
