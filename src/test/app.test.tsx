import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { AgentPicker, ApplyReview, ModelForm } from "../components";
import {
  newModel,
  type AppData,
  type ApplyPreview,
  type ModelTestResult,
} from "../types";
import { api } from "../api";
import { pickLocalPath } from "../file-picker";

vi.mock("../file-picker", () => ({ pickLocalPath: vi.fn() }));

vi.mock("../api", () => ({
  isDesktop: false,
  api: {
    data: vi.fn(),
    save: vi.fn(),
    testModel: vi.fn(),
    listModels: vi.fn(),
    delete: vi.fn(),
    settings: vi.fn(),
    preview: vi.fn(),
    apply: vi.fn(),
    cancel: vi.fn(),
    restore: vi.fn(),
    deleteBackup: vi.fn(),
    importPreview: vi.fn(),
    importConfirm: vi.fn(),
    share: vi.fn(),
  },
}));
const sample = {
  ...newModel(),
  id: "1",
  name: "我的模型",
  baseUrl: "https://api.example.com/v1",
  modelId: "test-model",
  apiKey: "secret-test-key",
};
const data: AppData = {
  models: [sample],
  settings: {
    theme: "system",
    workbuddyPath: null,
    claudePath: null,
    codexDir: null,
  },
  agents: [],
  dataDir: "/test/app",
  backups: [],
};
const preview: ApplyPreview = {
  token: "preview-1",
  title: "应用模型",
  files: [
    {
      path: "/test/models.json",
      before: "[]",
      after: '{"apiKey":"••••••••"}',
      fingerprint: "hash",
    },
  ],
  notices: ["请手动选择模型"],
};
const successfulTest: ModelTestResult = {
  message: "测试通过：已发送 test 并收到模型回复。",
  elapsedMs: 120,
};
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.data).mockResolvedValue(structuredClone(data));
  vi.mocked(api.cancel).mockResolvedValue();
  vi.mocked(api.testModel).mockReset().mockResolvedValue(successfulTest);
  vi.mocked(api.listModels)
    .mockReset()
    .mockResolvedValue(["test-model", "test-model-new"]);
});
afterEach(() => vi.useRealTimers());

describe("model editing", () => {
  it("opens the animated newcomer guide and leads into New API import", async () => {
    render(<App />);
    await screen.findByText("我的模型");
    const actions = screen
      .getAllByRole("button")
      .map((button) => button.textContent?.trim());
    expect(actions.indexOf("导入链接")).toBeLessThan(
      actions.indexOf("从 New API 添加"),
    );
    const guideButton = screen.getByRole("button", { name: "新手指引" });
    expect(guideButton.closest(".topbar")).not.toBeNull();
    await userEvent.click(guideButton);
    expect(
      screen.getByRole("heading", { name: "申请 WorkBuddy 密钥" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /下一步/ }));
    expect(
      screen.getByRole("heading", { name: "在模型库找到它" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /下一步/ }));
    expect(
      screen.getByRole("heading", { name: "应用到 Agent" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: /开始从 New API 添加/ }),
    );
    expect(
      await screen.findByRole("heading", { name: "从 New API 添加模型" }),
    ).toBeInTheDocument();
  });

  it("defaults image input on without changing an explicit saved off value", async () => {
    const props = { onSave: vi.fn(), onCancel: vi.fn(), busy: false };
    const { rerender } = render(<ModelForm initial={newModel()} {...props} />);
    await userEvent.click(screen.getByText("高级设置"));
    expect(screen.getByRole("checkbox", { name: "图像输入" })).toBeChecked();
    rerender(
      <ModelForm
        key="saved"
        initial={{ ...sample, supportsImages: false }}
        {...props}
      />,
    );
    await userEvent.click(screen.getByText("高级设置"));
    expect(
      screen.getByRole("checkbox", { name: "图像输入" }),
    ).not.toBeChecked();
  });
  it("masks the key and only saves on submit", async () => {
    const user = userEvent.setup();
    const save = vi.fn();
    render(
      <ModelForm
        initial={sample}
        onSave={save}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    expect(screen.getByDisplayValue("secret-test-key")).toHaveAttribute(
      "type",
      "password",
    );
    expect(save).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
    expect(api.testModel).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "显示密钥" }));
    expect(screen.getByDisplayValue("secret-test-key")).toHaveAttribute(
      "type",
      "text",
    );
    await user.click(screen.getByRole("button", { name: "测试模型" }));
    expect(api.testModel).toHaveBeenCalledWith(sample);
    expect(screen.getByRole("button", { name: "保存模型" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "保存模型" }));
    expect(save).toHaveBeenCalledWith(sample, successfulTest);
  });
  it("does not save when canceling", async () => {
    const cancel = vi.fn();
    const save = vi.fn();
    render(
      <ModelForm
        initial={sample}
        onSave={save}
        onCancel={cancel}
        busy={false}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(cancel).toHaveBeenCalledOnce();
    expect(save).not.toHaveBeenCalled();
  });
});

describe("model test gating and indicators", () => {
  it("prevents every submit until a successful test and blocks saving after a failed retest", async () => {
    const save = vi.fn();
    const { container } = render(
      <ModelForm
        initial={sample}
        onSave={save}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    fireEvent.submit(container.querySelector("form")!);
    expect(save).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "测试模型" }));
    expect(screen.getByRole("button", { name: "保存模型" })).toBeEnabled();
    vi.mocked(api.testModel).mockRejectedValueOnce(
      "测试未通过：HTTP 401，鉴权失败",
    );
    await userEvent.click(screen.getByRole("button", { name: "测试模型" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("HTTP 401");
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
    fireEvent.submit(container.querySelector("form")!);
    expect(save).not.toHaveBeenCalled();
  });

  it("requires another test after changing any draft configuration", async () => {
    render(
      <ModelForm
        initial={sample}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "测试模型" }));
    await user.type(screen.getByDisplayValue("secret-test-key"), "-changed");
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "刷新模型清单" }));
    await user.selectOptions(
      screen.getByRole("combobox", { name: "模型 ID" }),
      "test-model",
    );
    await user.click(screen.getByRole("button", { name: "测试模型" }));
    expect(api.testModel).toHaveBeenLastCalledWith({
      ...sample,
      apiKey: "secret-test-key-changed",
    });
    expect(screen.getByRole("button", { name: "保存模型" })).toBeEnabled();
    await user.click(screen.getByText("高级设置"));
    await user.click(screen.getByRole("checkbox", { name: "图像输入" }));
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
  });

  it("ignores an old response after editing during the request and does not double-send", async () => {
    let resolve!: (result: ModelTestResult) => void;
    vi.mocked(api.testModel).mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    render(
      <ModelForm
        initial={sample}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "测试模型" }));
    expect(screen.getByRole("button", { name: "测试中…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
    await userEvent.selectOptions(
      screen.getByRole("combobox", { name: "模型 ID" }),
      "test-model-new",
    );
    await act(async () => {
      resolve(successfulTest);
    });
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
    expect(screen.queryByText(/测试通过，耗时/)).not.toBeInTheDocument();
    expect(api.testModel).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByRole("button", { name: "测试模型" }));
    expect(screen.getByRole("button", { name: "保存模型" })).toBeEnabled();
  });

  it("does not let a canceled form response enable saving in a newly opened form", async () => {
    let resolve!: (result: ModelTestResult) => void;
    vi.mocked(api.testModel).mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    const first = render(
      <ModelForm
        initial={sample}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "测试模型" }));
    first.unmount();
    render(
      <ModelForm
        initial={sample}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    await act(async () => {
      resolve(successfulTest);
    });
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
    expect(screen.queryByText(/测试通过，耗时/)).not.toBeInTheDocument();
  });

  it("shows no initial marker, then green or red only after explicit card tests", async () => {
    render(<App />);
    await screen.findByText("我的模型");
    expect(
      screen.queryByRole("img", { name: /模型测试/ }),
    ).not.toBeInTheDocument();
    expect(api.testModel).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: "测试模型 我的模型" }),
    );
    expect(
      await screen.findByRole("img", { name: "模型测试通过" }),
    ).toHaveClass("passed");
    expect(screen.getByRole("img", { name: "模型测试通过" })).toHaveAttribute(
      "title",
      "测试通过，耗时0.12 秒",
    );
    vi.mocked(api.testModel).mockRejectedValueOnce("测试未通过：请求超时");
    await userEvent.click(
      screen.getByRole("button", { name: "测试模型 我的模型" }),
    );
    expect(
      await screen.findByRole("img", { name: "模型测试未通过" }),
    ).toHaveClass("failed");
    expect(
      screen.queryByRole("img", { name: "模型测试通过" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("请求超时");
    expect(api.save).not.toHaveBeenCalled();
    expect(api.apply).not.toHaveBeenCalled();
  });

  it("retains a successful form test on the saved card, while copies start untested", async () => {
    vi.mocked(api.save).mockResolvedValue(sample);
    render(<App />);
    await screen.findByText("我的模型");
    await userEvent.click(
      screen.getByRole("button", { name: "编辑 我的模型" }),
    );
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "测试模型" }));
    await userEvent.click(screen.getByRole("button", { name: "保存模型" }));
    expect(
      await screen.findByRole("img", { name: "模型测试通过" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "复制 我的模型" }),
    );
    expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
    expect(api.testModel).toHaveBeenCalledOnce();
  });

  it("ignores an earlier card request after saving and beginning a newer test", async () => {
    let rejectOld!: (reason: string) => void;
    let resolveNew!: (result: ModelTestResult) => void;
    vi.mocked(api.testModel)
      .mockReturnValueOnce(
        new Promise((_resolve, reject) => {
          rejectOld = reject;
        }),
      )
      .mockResolvedValueOnce(successfulTest)
      .mockReturnValueOnce(
        new Promise((resolve) => {
          resolveNew = resolve;
        }),
      );
    vi.mocked(api.save).mockResolvedValue(sample);
    render(<App />);
    await screen.findByText("我的模型");
    await userEvent.click(
      screen.getByRole("button", { name: "测试模型 我的模型" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "编辑 我的模型" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "测试模型" }));
    await userEvent.click(screen.getByRole("button", { name: "保存模型" }));
    await screen.findByRole("img", { name: "模型测试通过" });
    await userEvent.click(
      screen.getByRole("button", { name: "测试模型 我的模型" }),
    );
    await act(async () => {
      rejectOld("旧请求失败");
    });
    expect(
      screen.queryByRole("img", { name: "模型测试未通过" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "测试模型 我的模型" }),
    ).toBeDisabled();
    await act(async () => {
      resolveNew(successfulTest);
    });
    expect(
      screen.getByRole("img", { name: "模型测试通过" }),
    ).toBeInTheDocument();
  });
});

describe("temporary model test feedback", () => {
  it("hides success after exactly five seconds or manual dismissal without relocking save", async () => {
    vi.useFakeTimers();
    render(
      <ModelForm
        initial={sample}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    await act(async () => {});
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "测试模型" }));
    });
    expect(screen.getByText("测试通过，耗时0.12 秒")).toBeInTheDocument();
    expect(
      screen.queryByText(/已发送 test 并收到模型回复/),
    ).not.toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(4999);
    });
    expect(screen.getByText("测试通过，耗时0.12 秒")).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByText("测试通过，耗时0.12 秒")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存模型" })).toBeEnabled();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "测试模型" }));
    });
    fireEvent.click(screen.getByRole("button", { name: "关闭测试提示" }));
    expect(screen.queryByText("测试通过，耗时0.12 秒")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存模型" })).toBeEnabled();
  });

  it("expires card results, restarts the deadline on retest, and retains verification dots", async () => {
    vi.useFakeTimers();
    vi.mocked(api.testModel).mockRejectedValueOnce("测试未通过：HTTP 401");
    render(<App />);
    await act(async () => {});
    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: "测试模型 我的模型" }),
      );
    });
    expect(screen.getByText("测试未通过：HTTP 401")).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.queryByText("测试未通过：HTTP 401")).not.toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: "模型测试未通过" }),
    ).toBeInTheDocument();
    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: "测试模型 我的模型" }),
      );
    });
    await act(async () => {
      vi.advanceTimersByTime(3000);
    });
    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: "测试模型 我的模型" }),
      );
    });
    await act(async () => {
      vi.advanceTimersByTime(4999);
    });
    expect(screen.getByText("测试通过，耗时0.12 秒")).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(1);
    });
    expect(screen.queryByText("测试通过，耗时0.12 秒")).not.toBeInTheDocument();
    expect(
      screen.getByRole("img", { name: "模型测试通过" }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    fireEvent.click(screen.getByRole("button", { name: /模型库/ }));
    expect(screen.queryByText("测试通过，耗时0.12 秒")).not.toBeInTheDocument();
  });
});

describe("settings and backup controls", () => {
  it("keeps native selections in the draft until saving and preserves them on cancel", async () => {
    vi.mocked(api.data).mockResolvedValue({
      ...data,
      agents: [
        {
          agent: "workbuddy",
          path: "/home/.workbuddy/models.json",
          exists: true,
        },
        { agent: "claude", path: "/home/.claude/settings.json", exists: true },
        { agent: "codex", path: "/home/.codex/config.toml", exists: true },
      ],
    });
    vi.mocked(pickLocalPath)
      .mockResolvedValueOnce("/portable")
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce("/chosen/custom.json")
      .mockResolvedValueOnce("C:\\portable\\codex");
    vi.mocked(api.settings).mockResolvedValue();
    render(<App />);
    await screen.findByText("我的模型");
    await userEvent.click(screen.getByRole("button", { name: /设置/ }));
    await userEvent.click(
      screen.getByRole("button", { name: "选择 WorkBuddy 配置目录" }),
    );
    expect(
      screen.getByRole("textbox", { name: "WorkBuddy 配置路径" }),
    ).toHaveValue("/portable/models.json");
    await userEvent.click(
      screen.getByRole("button", { name: "选择 WorkBuddy 配置文件" }),
    );
    expect(
      screen.getByRole("textbox", { name: "WorkBuddy 配置路径" }),
    ).toHaveValue("/portable/models.json");
    await userEvent.click(
      screen.getByRole("button", { name: "选择 Claude Code 配置文件" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: /选择 Codex.*配置目录/ }),
    );
    expect(api.settings).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "保存设置" }));
    expect(api.settings).toHaveBeenCalledWith({
      ...data.settings,
      workbuddyPath: "/portable/models.json",
      claudePath: "/chosen/custom.json",
      codexDir: "C:\\portable\\codex",
    });
    expect(api.apply).not.toHaveBeenCalled();
  });

  it("only deletes the selected backup after the second confirmation", async () => {
    vi.mocked(api.data).mockResolvedValue({
      ...data,
      backups: [
        {
          id: "backup-1",
          title: "测试备份",
          createdAt: 1790035200,
          status: "complete",
          paths: ["/test/models.json"],
        },
      ],
    });
    vi.mocked(api.deleteBackup).mockResolvedValue();
    render(<App />);
    await screen.findByText("我的模型");
    await userEvent.click(screen.getByRole("button", { name: /模型配置备份/ }));
    await userEvent.click(
      screen.getByRole("button", { name: "删除备份 测试备份" }),
    );
    expect(api.deleteBackup).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(api.deleteBackup).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: "删除备份 测试备份" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "确认删除备份" }));
    expect(api.deleteBackup).toHaveBeenCalledWith("backup-1");
    expect(api.restore).not.toHaveBeenCalled();
    expect(api.apply).not.toHaveBeenCalled();
  });
});

describe("confirmation boundary", () => {
  it("disables incompatible Agent checkboxes", async () => {
    const next = vi.fn();
    render(<AgentPicker model={sample} onPreview={next} busy={false} />);
    expect(
      screen.getByRole("checkbox", { name: /Claude Code/ }),
    ).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /Codex/ })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "预览变更" }));
    expect(
      screen.getByRole("checkbox", {
        name: /写入后打开 WorkBuddy 新建任务页/,
      }),
    ).toBeChecked();
    expect(next).toHaveBeenCalledWith(["workbuddy"], true);
  });
  it("allows opting out and resets the default after reselecting WorkBuddy", async () => {
    const next = vi.fn();
    render(<AgentPicker model={sample} onPreview={next} busy={false} />);
    const workbuddy = screen.getByRole("checkbox", {
      name: /^WorkBuddy 加入模型列表/,
    });
    const autoSelect = screen.getByRole("checkbox", {
      name: /写入后打开 WorkBuddy 新建任务页/,
    });
    await userEvent.click(autoSelect);
    await userEvent.click(screen.getByRole("button", { name: "预览变更" }));
    expect(next).toHaveBeenLastCalledWith(["workbuddy"], false);
    await userEvent.click(workbuddy);
    expect(autoSelect).toBeDisabled();
    expect(autoSelect).not.toBeChecked();
    await userEvent.click(workbuddy);
    expect(autoSelect).toBeChecked();
    await userEvent.click(screen.getByRole("button", { name: "预览变更" }));
    expect(next).toHaveBeenLastCalledWith(["workbuddy"], true);
  });
  it("shows the WorkBuddy post-write action in the second confirmation", () => {
    render(
      <ApplyReview
        preview={{
          ...preview,
          notices: [
            "配置成功写入后，将唤起 WorkBuddy 新建任务页并预选该模型。",
          ],
        }}
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    expect(screen.getByText(/配置成功写入后，将唤起 WorkBuddy/)).toBeVisible();
  });
  it("requires the explicit second confirmation", async () => {
    const confirm = vi.fn();
    const cancel = vi.fn();
    render(
      <ApplyReview
        preview={preview}
        onConfirm={confirm}
        onCancel={cancel}
        busy={false}
      />,
    );
    expect(confirm).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(confirm).not.toHaveBeenCalled();
    expect(cancel).toHaveBeenCalledOnce();
    await userEvent.click(
      screen.getByRole("button", { name: "确认覆盖并备份" }),
    );
    expect(confirm).toHaveBeenCalledOnce();
  });
  it("never writes while opening and canceling a preview", async () => {
    vi.mocked(api.preview).mockResolvedValue(preview);
    render(<App />);
    await screen.findByText("我的模型");
    await userEvent.click(screen.getByRole("button", { name: "应用到 Agent" }));
    await userEvent.click(screen.getByRole("button", { name: "预览变更" }));
    await screen.findByRole("button", { name: "确认覆盖并备份" });
    expect(api.apply).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(api.cancel).toHaveBeenCalledWith("preview-1");
    expect(api.apply).not.toHaveBeenCalled();
  });
});

describe("library and imports", () => {
  it("filters locally and hides list credentials", async () => {
    render(<App />);
    await screen.findByText("我的模型");
    expect(screen.queryByText("secret-test-key")).not.toBeInTheDocument();
    await userEvent.type(
      screen.getByRole("textbox", { name: "搜索模型" }),
      "absent",
    );
    expect(screen.getByText("没有找到匹配的模型")).toBeInTheDocument();
  });
  it("previews links without importing or applying automatically", async () => {
    vi.mocked(api.importPreview).mockResolvedValue({
      token: "import-1",
      rows: [
        {
          index: 0,
          name: "导入样例",
          modelId: "new",
          protocol: "openai-chat",
          baseUrl: "https://example.com",
          hasApiKey: true,
          duplicate: true,
        },
      ],
    });
    render(<App />);
    await screen.findByText("我的模型");
    await userEvent.click(screen.getByRole("button", { name: "导入链接" }));
    await userEvent.type(
      screen.getByRole("textbox", { name: "模型链接" }),
      "power-switch://model/import?v=1&data=test",
    );
    await userEvent.click(screen.getByRole("button", { name: "预览导入" }));
    await screen.findByText("导入样例");
    expect(api.importConfirm).not.toHaveBeenCalled();
    expect(api.apply).not.toHaveBeenCalled();
    expect(
      screen.getByRole("checkbox", { name: "更新重复项" }),
    ).not.toBeChecked();
  });
  it("shares without a key unless explicitly requested", async () => {
    vi.mocked(api.share).mockResolvedValue(
      "power-switch://model/import?v=1&data=safe",
    );
    render(<App />);
    await screen.findByText("我的模型");
    await userEvent.click(
      screen.getByRole("button", { name: "分享 我的模型" }),
    );
    await waitFor(() => expect(api.share).toHaveBeenCalledWith("1", false));
    await userEvent.click(
      screen.getByRole("checkbox", { name: /包含 API Key/ }),
    );
    await waitFor(() => expect(api.share).toHaveBeenCalledWith("1", true));
  });
});

describe("provider model selector", () => {
  it("places a required API Key before model ID and blocks empty-key fetches in both forms", async () => {
    for (const initial of [newModel(), { ...sample, apiKey: "" }]) {
      const { container, unmount } = render(
        <ModelForm
          initial={initial}
          onSave={vi.fn()}
          onCancel={vi.fn()}
          busy={false}
        />,
      );
      const key = screen.getByLabelText(/API Key/) as HTMLInputElement;
      const select = screen.getByRole("combobox", { name: "模型 ID" });
      expect(key.required).toBe(true);
      expect(
        key.compareDocumentPosition(select) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      expect(
        screen.getByRole("button", {
          name: initial.id ? "刷新模型清单" : "获取模型清单",
        }),
      ).toBeDisabled();
      expect(screen.getByRole("button", { name: "测试模型" })).toBeDisabled();
      expect(screen.getByRole("button", { name: "保存模型" })).toBeDisabled();
      expect(container).toHaveTextContent("请先填写 API Key");
      unmount();
    }
    expect(api.listModels).not.toHaveBeenCalled();
  });

  it("uses a platform name and only accepts an ID returned by the provider when adding", async () => {
    vi.mocked(api.listModels).mockResolvedValueOnce([
      "platform-model",
      "platform-model-mini",
    ]);
    const save = vi.fn();
    const user = userEvent.setup();
    render(
      <ModelForm
        initial={newModel()}
        onSave={save}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    expect(screen.getByRole("textbox", { name: "平台名称" })).toBeVisible();
    expect(screen.queryByRole("textbox", { name: "模型 ID" })).toBeNull();
    const select = screen.getByRole("combobox", { name: "模型 ID" });
    expect(select).toBeDisabled();
    await user.type(
      screen.getByRole("textbox", { name: "平台名称" }),
      "winwin",
    );
    await user.type(
      screen.getByRole("textbox", { name: /API 地址/ }),
      "https://api.example.com/v1",
    );
    await user.type(screen.getByLabelText(/API Key/), "test-key");
    await user.click(screen.getByRole("button", { name: "获取模型清单" }));
    expect(
      await screen.findByRole("option", { name: "platform-model" }),
    ).toBeInTheDocument();
    expect(api.listModels).toHaveBeenCalledWith({
      protocol: "openai-chat",
      baseUrl: "https://api.example.com/v1",
      apiKey: "test-key",
    });
    await user.selectOptions(select, "platform-model");
    await user.click(screen.getByRole("button", { name: "测试模型" }));
    const saved = {
      ...newModel(),
      name: "winwin · platform-model",
      baseUrl: "https://api.example.com/v1",
      modelId: "platform-model",
      apiKey: "test-key",
    };
    expect(api.testModel).toHaveBeenCalledWith(saved);
    await user.click(screen.getByRole("button", { name: "保存模型" }));
    expect(save).toHaveBeenCalledWith(saved, successfulTest);
  });

  it("loads platform IDs and requires selecting an available model", async () => {
    vi.mocked(api.listModels).mockResolvedValueOnce(["platform-model"]);
    render(
      <ModelForm
        initial={sample}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    const select = screen.getByRole("combobox", { name: "模型 ID" });
    expect(select).toBeDisabled();
    await screen.findByRole("option", { name: "platform-model" });
    expect(api.listModels).toHaveBeenCalledWith({
      protocol: sample.protocol,
      baseUrl: sample.baseUrl,
      apiKey: sample.apiKey,
    });
    expect(screen.getByRole("button", { name: "测试模型" })).toBeDisabled();
    await userEvent.selectOptions(select, "platform-model");
    await userEvent.click(screen.getByRole("button", { name: "测试模型" }));
    expect(api.testModel).toHaveBeenCalledWith({
      ...sample,
      modelId: "platform-model",
    });
    expect(screen.getByRole("button", { name: "保存模型" })).toBeEnabled();
  });

  it("keeps selection unavailable on failure and supports explicit retry", async () => {
    vi.mocked(api.listModels).mockRejectedValueOnce(
      "获取模型清单失败（HTTP 401）",
    );
    render(
      <ModelForm
        initial={sample}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    await screen.findByText("获取模型清单失败（HTTP 401）");
    expect(screen.getByRole("combobox", { name: "模型 ID" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "测试模型" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "刷新模型清单" }));
    expect(screen.getByRole("combobox", { name: "模型 ID" })).toHaveValue(
      "test-model",
    );
  });

  it("discards a stale catalog after connection edits", async () => {
    let resolve!: (ids: string[]) => void;
    vi.mocked(api.listModels).mockReturnValueOnce(
      new Promise((done) => {
        resolve = done;
      }),
    );
    render(
      <ModelForm
        initial={sample}
        onSave={vi.fn()}
        onCancel={vi.fn()}
        busy={false}
      />,
    );
    await userEvent.type(screen.getByDisplayValue("secret-test-key"), "-new");
    await act(async () => resolve(["stale-model"]));
    expect(
      screen.queryByRole("option", { name: "stale-model" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "测试模型" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "刷新模型清单" }));
    expect(api.listModels).toHaveBeenLastCalledWith({
      protocol: sample.protocol,
      baseUrl: sample.baseUrl,
      apiKey: "secret-test-key-new",
    });
    expect(screen.getByRole("combobox", { name: "模型 ID" })).toHaveValue("");
  });
});
