import { isTauri } from "@tauri-apps/api/core";

/** Open the native OS picker; cancel returns null and never changes saved settings. */
export async function pickLocalPath(
  kind: "file" | "directory",
  title: string,
  current?: string,
): Promise<string | null> {
  if (!isTauri()) throw new Error("选择本地文件或目录需要在桌面应用中使用。");
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    directory: kind === "directory",
    multiple: false,
    title,
    defaultPath: current || undefined,
  });
  return typeof selected === "string" ? selected : null;
}
