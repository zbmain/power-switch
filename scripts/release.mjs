import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { createReadStream } from "node:fs";
import {
  copyFile,
  lstat,
  mkdir,
  readdir,
  readFile,
  writeFile,
} from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export const targets = {
  "macos-universal": {
    triple: "universal-apple-darwin",
    extensions: ["dmg", "zip"],
  },
  "windows-x64": {
    triple: "x86_64-pc-windows-msvc",
    extensions: ["msi", "zip"],
  },
  "windows-arm64": {
    triple: "aarch64-pc-windows-msvc",
    extensions: ["msi", "zip"],
  },
  "linux-x64": {
    triple: "x86_64-unknown-linux-gnu",
    extensions: ["AppImage", "deb", "rpm"],
  },
  "linux-arm64": {
    triple: "aarch64-unknown-linux-gnu",
    extensions: ["AppImage", "deb", "rpm"],
  },
};

/** 校验 v 前缀 SemVer，拒绝数字标识符前导零及不安全的文件名字符。 */
export function versionFromTag(tag) {
  const match =
    /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?$/.exec(
      tag ?? "",
    );
  if (
    !match ||
    match[0] !== tag ||
    match[4]?.split(".").some((part) => /^\d+$/.test(part) && /^0\d/.test(part))
  ) {
    throw new Error(`Invalid release tag: ${tag}. Expected v<SemVer>.`);
  }
  return tag.slice(1);
}

/** 读取本项目普通 TOML package 表中的字符串；不推断缺失或继承的版本。 */
export function packageVersion(toml, lockfile = false) {
  const heading = lockfile ? "[[package]]" : "[package]";
  const sections = toml
    .split(/(?=^\[)/m)
    .filter((section) => section.split(/\r?\n/, 1)[0].trim() === heading);
  const own = sections.filter((section) =>
    /^name\s*=\s*"power-switch"\s*(?:#.*)?$/m.test(section),
  );
  if (own.length !== 1)
    throw new Error("Expected exactly one power-switch package table");
  const version = /^version\s*=\s*"([^"\r\n]+)"\s*(?:#.*)?$/m.exec(own[0]);
  if (!version)
    throw new Error("Missing explicit power-switch package version");
  return version[1];
}

/** 比对前端、Tauri、Cargo 清单和 Cargo 锁文件中的项目版本。 */
export async function checkVersion(root, tag) {
  const version = versionFromTag(tag);
  const files = [
    "package.json",
    "src-tauri/tauri.conf.json",
    "src-tauri/Cargo.toml",
    "src-tauri/Cargo.lock",
  ];
  for (const file of files) {
    const contents = await readFile(join(root, file), "utf8");
    const actual = file.endsWith(".json")
      ? JSON.parse(contents).version
      : packageVersion(contents, file.endsWith(".lock"));
    if (actual !== version)
      throw new Error(`${file}: version ${actual} does not match ${tag}`);
  }
  return version;
}

/** 生成唯一且包含版本、系统、架构的发布资产名称。 */
export function expectedAssets(tag, platform) {
  versionFromTag(tag);
  if (platform && !targets[platform])
    throw new Error(`Unknown release platform: ${platform}`);
  return Object.entries(targets)
    .filter(([name]) => !platform || name === platform)
    .flatMap(([name, target]) =>
      target.extensions.map(
        (extension) => `power-switch-${tag}-${name}.${extension}`,
      ),
    )
    .sort();
}

/** 发布前要求目录中恰好包含所有预期非空普通文件，拒绝软链接和多余文件。 */
export async function validateAssets(directory, tag) {
  const expected = expectedAssets(tag);
  const actual = (await readdir(directory))
    .filter((name) => name !== "SHA256SUMS")
    .sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected))
    throw new Error(
      `Release assets mismatch. Expected: ${expected.join(", ")}; found: ${actual.join(", ")}`,
    );
  for (const name of expected) {
    const stat = await lstat(join(directory, name));
    if (!stat.isFile() || stat.size === 0)
      throw new Error(`Release asset is not a nonempty regular file: ${name}`);
  }
  return expected;
}

/** 流式计算大安装包的 SHA-256，避免将整个文件载入内存。 */
export async function sha256(path) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(path)) hash.update(chunk);
  return hash.digest("hex");
}

/** 在全部平台产物验证通过后写入标准 SHA256SUMS。 */
export async function writeChecksums(directory, tag) {
  const names = await validateAssets(directory, tag);
  const lines = [];
  for (const name of names)
    lines.push(`${await sha256(join(directory, name))}  ${name}`);
  await writeFile(join(directory, "SHA256SUMS"), `${lines.join("\n")}\n`);
  return names;
}

/** 查找指定扩展名的唯一产物；缺包或多包均失败，避免上传错误架构。 */
async function bundleFile(directory, extension) {
  const matches = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory())
      matches.push(...(await matchingFiles(path, extension)));
    else if (entry.isFile() && entry.name.endsWith(`.${extension}`))
      matches.push(path);
  }
  if (matches.length !== 1)
    throw new Error(
      `Expected one .${extension} bundle in ${directory}; found ${matches.length}`,
    );
  return matches[0];
}

/** 递归枚举指定类型的构建文件，不跟随软链接。 */
async function matchingFiles(directory, extension) {
  const matches = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory())
      matches.push(...(await matchingFiles(path, extension)));
    else if (entry.isFile() && entry.name.endsWith(`.${extension}`))
      matches.push(path);
  }
  return matches;
}

/** 从显式 Rust target 目录收集本平台安装包，并制作 macOS/Windows ZIP。 */
export async function collectAssets(root, directory, tag, platform) {
  await checkVersion(root, tag);
  const names = expectedAssets(tag, platform);
  const base = join(
    root,
    "src-tauri/target",
    targets[platform].triple,
    "release",
  );
  await mkdir(directory, { recursive: true });
  for (const name of names) {
    const extension = name.split(".").at(-1);
    const output = join(directory, name);
    if (extension !== "zip") {
      await copyFile(await bundleFile(join(base, "bundle"), extension), output);
    } else if (platform === "macos-universal") {
      const app = join(base, "bundle/macos/power-switch.app");
      execFileSync(
        "lipo",
        [
          "-verify_arch",
          "x86_64",
          "arm64",
          join(app, "Contents/MacOS/power-switch"),
        ],
        { stdio: "inherit" },
      );
      execFileSync(
        "ditto",
        ["-c", "-k", "--sequesterRsrc", "--keepParent", app, output],
        { stdio: "inherit" },
      );
    } else {
      execFileSync(
        "pwsh",
        [
          "-NoProfile",
          "-NonInteractive",
          "-Command",
          "$ErrorActionPreference = 'Stop'; Compress-Archive -LiteralPath $env:RELEASE_EXE -DestinationPath $env:RELEASE_ZIP -Force",
        ],
        {
          stdio: "inherit",
          env: {
            ...process.env,
            RELEASE_EXE: join(base, "power-switch.exe"),
            RELEASE_ZIP: output,
          },
        },
      );
    }
    const stat = await lstat(output);
    if (!stat.isFile() || stat.size === 0)
      throw new Error(`Empty bundle: ${output}`);
  }
}

/** 提供版本校验、平台产物收集和统一校验和生成三个 CLI 子命令。 */
async function main() {
  const [command, tag, platform] = process.argv
    .slice(2)
    .filter((arg) => arg !== "--");
  const root = resolve(
    process.env.RELEASE_ROOT ?? fileURLToPath(new URL("..", import.meta.url)),
  );
  const directory = join(root, "release-assets");
  if (command === "check") await checkVersion(root, tag);
  else if (command === "collect" && platform)
    await collectAssets(root, directory, tag, platform);
  else if (command === "checksums") await writeChecksums(directory, tag);
  else
    throw new Error(
      "Usage: release.mjs check|collect|checksums v<version> [platform]",
    );
  console.log(`Release ${command} passed: ${tag}`);
}

if (
  process.argv[1] &&
  resolve(process.argv[1]) === fileURLToPath(import.meta.url)
) {
  main().catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
