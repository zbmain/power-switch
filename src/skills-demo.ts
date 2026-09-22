import type { SkillGroup } from "./skills-api";

const groups: SkillGroup[] = [
  {
    scope: "global",
    path: "用户目录/.agents/skills",
    error: null,
    skills: [
      {
        id: "code-review",
        name: "code-review",
        description:
          "检查代码的正确性、可维护性与测试覆盖，提供清晰、可执行的建议。",
        path: "用户目录/.agents/skills/code-review",
        linked: false,
        linkTarget: null,
        problem: null,
      },
      {
        id: "research-notes",
        name: "research-notes",
        description: "整理资料与引用，将研究过程沉淀成结构清晰的笔记。",
        path: "用户目录/.agents/skills/research-notes",
        linked: false,
        linkTarget: null,
        problem: null,
      },
    ],
  },
  { scope: "claude", path: "用户目录/.claude/skills", error: null, skills: [] },
  { scope: "codex", path: "用户目录/.codex/skills", error: null, skills: [] },
  {
    scope: "workbuddy",
    path: "用户目录/.workbuddy/skills",
    error: null,
    skills: [],
  },
];

/** Render fictional examples only; browser mode cannot install, link, or remove real skills. */
export async function demoSkills(
  command: string,
  args?: Record<string, unknown>,
): Promise<unknown> {
  if (command === "skills_list") return structuredClone(groups);
  if (command === "skills_detail")
    return {
      document:
        "---\nname: " +
        args?.id +
        "\ndescription: 浏览器演示技能\n---\n\n# 使用说明\n\n这是用于展示界面的示例技能。\n实际技能内容在桌面应用中读取。",
      path: "用户目录/.agents/skills/" + args?.id + "/SKILL.md",
    };
  if (command === "skills_cancel") return;
  throw new Error("技能安装、删除和软链接操作请在桌面应用中使用。");
}
