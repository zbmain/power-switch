import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { AgentPicker, ApplyReview, ModelForm } from "../components";
import { newModel, type AppData, type ApplyPreview } from "../types";
import { api } from "../api";
import { pickLocalPath } from "../file-picker";

vi.mock("../file-picker", () => ({ pickLocalPath: vi.fn() }));

vi.mock("../api", () => ({
  isDesktop: false,
  api: {
    data: vi.fn(),
    save: vi.fn(),
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
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.data).mockResolvedValue(structuredClone(data));
  vi.mocked(api.cancel).mockResolvedValue();
});

describe("model editing", () => {
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
    await user.click(screen.getByRole("button", { name: "显示密钥" }));
    expect(screen.getByDisplayValue("secret-test-key")).toHaveAttribute(
      "type",
      "text",
    );
    await user.click(screen.getByRole("button", { name: "保存模型" }));
    expect(save).toHaveBeenCalledWith(sample);
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
    await userEvent.click(screen.getByRole("button", { name: /模型配置记录/ }));
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
    expect(next).toHaveBeenCalledWith(["workbuddy"]);
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
