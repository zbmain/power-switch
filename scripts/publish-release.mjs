import { execFileSync } from "node:child_process";
import { readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { checkVersion, sha256, writeChecksums } from "./release.mjs";

/** 禁止修改正式版；已有预发布只允许同一提交的幂等重跑。 */
export function assertPublishable(release, commit) {
  if (!release) return;
  if (!release.draft && !release.prerelease)
    throw new Error(
      "Refusing to overwrite an already published stable release",
    );
  if (release.target_commitish !== commit)
    throw new Error("Release already belongs to a different commit");
}

/** 调用 GitHub API；认证仅通过环境变量传递，错误不输出令牌或请求头。 */
async function api(path, options = {}) {
  const response = await fetch(`https://api.github.com${path}`, {
    ...options,
    headers: {
      Accept: "application/vnd.github+json",
      Authorization: `Bearer ${process.env.GH_TOKEN}`,
      "X-GitHub-Api-Version": "2022-11-28",
      "Content-Type": "application/json",
      ...options.headers,
    },
  });
  if (response.status === 404 && options.allowMissing) return null;
  if (!response.ok)
    throw new Error(
      `GitHub API ${options.method ?? "GET"} ${path}: HTTP ${response.status}`,
    );
  return response.status === 204 ? null : response.json();
}

/** 校验 GitHub 已接收的全部资产名称、大小和服务端 SHA-256。 */
async function verifyUploads(repository, releaseId, directory, names) {
  const assets = await api(
    `/repos/${repository}/releases/${releaseId}/assets?per_page=100`,
  );
  if (
    JSON.stringify(assets.map((asset) => asset.name).sort()) !==
    JSON.stringify([...names].sort())
  )
    throw new Error("Uploaded release asset list is incomplete or unexpected");
  for (const asset of assets) {
    const digest = await sha256(join(directory, asset.name));
    if (asset.digest !== `sha256:${digest}` || asset.size <= 0)
      throw new Error(`Uploaded asset checksum mismatch: ${asset.name}`);
  }
}

/** 通过草稿完成资产上传与验证，全部成功后才对用户公开为预发布。 */
async function main() {
  const repository = process.env.GITHUB_REPOSITORY;
  const tag = process.env.GITHUB_REF_NAME;
  const commit = process.env.GITHUB_SHA;
  if (
    !process.env.GH_TOKEN ||
    !/^[\w.-]+\/[\w.-]+$/.test(repository ?? "") ||
    !/^[a-f0-9]{40}$/.test(commit ?? "")
  )
    throw new Error("Missing or invalid GitHub release environment");
  const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
  const directory = join(root, "release-assets");
  await checkVersion(root, tag);
  const names = [...(await writeChecksums(directory, tag)), "SHA256SUMS"];
  const base = `/repos/${repository}/releases`;
  let release = await api(`${base}/tags/${encodeURIComponent(tag)}`, {
    allowMissing: true,
  });
  assertPublishable(release, commit);
  if (release && !release.draft) {
    await verifyUploads(repository, release.id, directory, names);
    console.log(
      `Existing prerelease is complete and unchanged: ${release.html_url}`,
    );
    return;
  }
  if (!release) {
    release = await api(base, {
      method: "POST",
      body: JSON.stringify({
        tag_name: tag,
        target_commitish: commit,
        name: `power-switch ${tag}`,
        body: await readFile(join(root, ".github/RELEASE_TEMPLATE.md"), "utf8"),
        draft: true,
        prerelease: true,
        make_latest: "false",
      }),
    });
  }
  execFileSync(
    "gh",
    [
      "release",
      "upload",
      tag,
      ...names.map((name) => join(directory, name)),
      "--repo",
      repository,
      "--clobber",
    ],
    { stdio: "inherit" },
  );
  await verifyUploads(repository, release.id, directory, names);
  release = await api(`${base}/${release.id}`);
  assertPublishable(release, commit);
  if (!release.draft)
    throw new Error("Release state changed during upload; refusing to publish");
  const result = await api(`${base}/${release.id}`, {
    method: "PATCH",
    body: JSON.stringify({
      draft: false,
      prerelease: true,
      make_latest: "false",
    }),
  });
  console.log(`Published prerelease: ${result.html_url}`);
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
