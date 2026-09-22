import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SkillsPage } from "../SkillsPage";
import { skillsApi, type SkillGroup } from "../skills-api";

vi.mock("../skills-api", () => ({
  scopeLabels: {
    global: "全局",
    claude: "Claude Code",
    codex: "Codex",
    workbuddy: "WorkBuddy",
  },
  skillsApi: {
    list: vi.fn(),
    detail: vi.fn(),
    installPreview: vi.fn(),
    linkPreview: vi.fn(),
    deletePreview: vi.fn(),
    commit: vi.fn(),
    cancel: vi.fn(),
  },
}));
vi.mock("../file-picker", () => ({ pickLocalPath: vi.fn() }));
const groups: SkillGroup[] = [
  {
    scope: "global",
    path: "/home/.agents/skills",
    error: null,
    skills: [
      {
        id: "example",
        name: "示例技能",
        description: "处理文档",
        path: "/home/.agents/skills/example",
        linked: false,
        linkTarget: null,
        problem: null,
      },
    ],
  },
  {
    scope: "claude",
    path: "/home/.claude/skills",
    error: null,
    skills: [
      {
        id: "linked",
        name: "已连接技能",
        description: "处理文档",
        path: "/home/.claude/skills/linked",
        linked: true,
        linkTarget: "/home/.agents/skills/example",
        problem: null,
      },
    ],
  },
  { scope: "codex", path: "/home/.codex/skills", error: null, skills: [] },
  {
    scope: "workbuddy",
    path: "/home/.workbuddy/skills",
    error: null,
    skills: [],
  },
];
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(skillsApi.list).mockResolvedValue(structuredClone(groups));
  vi.mocked(skillsApi.detail).mockResolvedValue({
    document: "# 技能正文\n<script>不执行</script>",
    path: "/fixture/SKILL.md",
  });
  vi.mocked(skillsApi.cancel).mockResolvedValue();
  vi.mocked(skillsApi.commit).mockResolvedValue("操作完成");
});
afterEach(() => {
  vi.useRealTimers();
});

describe("skill inventories", () => {
  it("shows scoped directories, linked details and read-only untrusted Markdown", async () => {
    render(<SkillsPage />);
    await screen.findByRole("option", { name: /示例技能/ });
    expect(screen.getByText("/home/.agents/skills")).toBeInTheDocument();
    expect(
      await screen.findByText(/<script>不执行<\/script>/),
    ).toBeInTheDocument();
    expect(document.querySelector("script")).toBeNull();
    await userEvent.click(screen.getByRole("tab", { name: /Claude Code/ }));
    await screen.findByRole("option", { name: /已连接技能/ });
    expect(screen.getByText("/home/.claude/skills")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "安装技能" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "连接到 Agent" }),
    ).not.toBeInTheDocument();
    await userEvent.type(
      screen.getByRole("textbox", { name: "搜索技能" }),
      "unknown",
    );
    expect(screen.getByText("没有匹配的技能")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "前往全局安装" }));
    expect(
      screen.getByRole("button", { name: "安装技能" }),
    ).toBeInTheDocument();
  });

  it("refreshes every minute and starts a new minute on manual refresh", async () => {
    vi.useFakeTimers();
    const { unmount } = render(<SkillsPage />);
    await act(async () => {});
    expect(skillsApi.list).toHaveBeenCalledTimes(1);
    await act(async () => {
      vi.advanceTimersByTime(30000);
    });
    expect(screen.getByText("30 秒后自动刷新")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "刷新" }));
    await act(async () => {});
    expect(skillsApi.list).toHaveBeenCalledTimes(2);
    await act(async () => {
      vi.advanceTimersByTime(59999);
    });
    expect(skillsApi.list).toHaveBeenCalledTimes(2);
    await act(async () => {
      vi.advanceTimersByTime(1);
    });
    expect(skillsApi.list).toHaveBeenCalledTimes(3);
    unmount();
    await act(async () => {
      vi.advanceTimersByTime(120000);
    });
    expect(skillsApi.list).toHaveBeenCalledTimes(3);
  });

  it("does not overlap scans when a filesystem is slow", async () => {
    vi.useFakeTimers();
    let finish!: (value: SkillGroup[]) => void;
    vi.mocked(skillsApi.list).mockReturnValueOnce(
      new Promise((resolve) => {
        finish = resolve;
      }),
    );
    const { unmount } = render(<SkillsPage />);
    await act(async () => {
      vi.advanceTimersByTime(90000);
    });
    expect(skillsApi.list).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: "刷新" })).toBeDisabled();
    await act(async () => {
      finish(groups);
    });
    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(skillsApi.list).toHaveBeenCalledTimes(2);
    unmount();
  });
});

describe("skill confirmation boundaries", () => {
  it("previews installs globally and cancel disposes staging without committing", async () => {
    vi.mocked(skillsApi.installPreview).mockResolvedValue({
      token: "install-1",
      destination: "/home/.agents/skills",
      skills: [
        {
          index: 0,
          name: "新技能",
          description: "新描述",
          folder: "new",
          exists: false,
        },
        {
          index: 1,
          name: "已有技能",
          description: "",
          folder: "old",
          exists: true,
        },
      ],
    });
    render(<SkillsPage />);
    await screen.findByRole("option", { name: /示例技能/ });
    await userEvent.click(screen.getByRole("button", { name: "安装技能" }));
    await userEvent.type(
      screen.getByRole("textbox", { name: "技能目录或 SKILL.md" }),
      "/fixture/skills",
    );
    await userEvent.click(screen.getByRole("button", { name: "预览安装" }));
    const dialog = await screen.findByRole("dialog", { name: "确认安装技能" });
    expect(skillsApi.installPreview).toHaveBeenCalledWith({
      kind: "local",
      path: "/fixture/skills",
    });
    expect(
      within(dialog).getByRole("checkbox", { name: /已有技能/ }),
    ).toBeDisabled();
    expect(skillsApi.commit).not.toHaveBeenCalled();
    await userEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(skillsApi.cancel).toHaveBeenCalledWith("install-1");
    expect(skillsApi.commit).not.toHaveBeenCalled();
  });

  it("commits only selected reviewed install candidates", async () => {
    vi.mocked(skillsApi.installPreview).mockResolvedValue({
      token: "install-2",
      destination: "/home/.agents/skills",
      skills: [
        {
          index: 4,
          name: "Git技能",
          description: "",
          folder: "git-skill",
          exists: false,
        },
      ],
    });
    render(<SkillsPage />);
    await screen.findByRole("option", { name: /示例技能/ });
    await userEvent.click(screen.getByRole("button", { name: "安装技能" }));
    await userEvent.click(screen.getByRole("button", { name: "Git 仓库" }));
    await userEvent.type(
      screen.getByRole("textbox", { name: /仓库.*链接|仓库地址/ }),
      "https://example.com/repo.git",
    );
    await userEvent.click(screen.getByRole("button", { name: "预览安装" }));
    await userEvent.click(
      await screen.findByRole("button", { name: "确认安装到全局" }),
    );
    expect(skillsApi.commit).toHaveBeenCalledWith("install-2", [4]);
    await screen.findByText("操作完成");
  });

  it("connects global skills to selected Agents only after reviewing paths", async () => {
    vi.mocked(skillsApi.linkPreview).mockResolvedValue({
      token: "link-1",
      title: "确认建立软链接",
      message: "将建立 1 个软链接",
      paths: ["/home/.claude/skills/example"],
    });
    render(<SkillsPage />);
    await screen.findByRole("option", { name: /示例技能/ });
    await userEvent.click(screen.getByRole("button", { name: "连接到 Agent" }));
    await userEvent.click(
      screen.getByRole("checkbox", { name: "Claude Code" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "预览软链接" }));
    await screen.findByText("/home/.claude/skills/example");
    expect(skillsApi.linkPreview).toHaveBeenCalledWith("example", ["claude"]);
    expect(skillsApi.commit).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: "确认建立软链接" }),
    );
    expect(skillsApi.commit).toHaveBeenCalledWith("link-1", []);
  });

  it("requires a second deletion confirmation and invalidates the dialog on cancellation", async () => {
    vi.mocked(skillsApi.deletePreview).mockResolvedValue({
      token: "delete-1",
      title: "删除技能",
      message: "同时移除引用",
      paths: ["/home/.agents/skills/example", "/home/.claude/skills/example"],
    });
    render(<SkillsPage />);
    await screen.findByRole("option", { name: /示例技能/ });
    await userEvent.click(screen.getByRole("button", { name: "删除技能" }));
    await screen.findByRole("button", { name: "确认删除技能" });
    expect(skillsApi.commit).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(skillsApi.cancel).toHaveBeenCalledWith("delete-1");
    expect(skillsApi.commit).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "删除技能" }));
    await userEvent.click(
      await screen.findByRole("button", { name: "确认删除技能" }),
    );
    await waitFor(() =>
      expect(skillsApi.commit).toHaveBeenCalledWith("delete-1", []),
    );
  });
});
