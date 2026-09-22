import { spawn, execFileSync } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

/** Validate a generated catalog using the installed Codex parser without touching the user's CODEX_HOME. */
async function main() {
  const root = await mkdtemp(join(tmpdir(), "power-switch-codex-"));
  execFileSync(
    "cargo",
    [
      "run",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--no-default-features",
      "--example",
      "acceptance",
      "--",
      "codex",
      root,
    ],
    { stdio: "inherit" },
  );
  const child = spawn(
    process.env.CODEX_BIN || "codex",
    ["app-server", "--stdio", "--strict-config"],
    {
      env: { ...process.env, CODEX_HOME: join(root, ".codex") },
      stdio: ["pipe", "pipe", "pipe"],
    },
  );
  /** Send one JSON-RPC request on Codex's local stdio transport. */
  const send = (message) => child.stdin.write(JSON.stringify(message) + "\n");
  try {
    await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("Codex parser timed out")),
        20000,
      );
      /** Resolve exactly once and release the verification timeout. */
      const done = (error) => {
        clearTimeout(timer);
        error ? reject(error) : resolve();
      };
      child.on("error", done);
      child.on("exit", () =>
        done(new Error("Codex exited before verification completed")),
      );
      createInterface({ input: child.stdout }).on("line", (line) => {
        try {
          const response = JSON.parse(line);
          if (response.error)
            throw new Error("Codex rejected generated configuration");
          if (response.id === 1) {
            send({ method: "initialized", params: {} });
            send({
              id: 2,
              method: "model/list",
              params: { includeHidden: true },
            });
          }
          if (response.id === 2) {
            const found = response.result.data.find(
              (m) => m.model === "power-switch-acceptance" && m.isDefault,
            );
            if (!found)
              throw new Error("Generated model was not recognized as default");
            console.log(
              "Codex parsed the generated provider and catalog; acceptance model is default.",
            );
            done();
          }
        } catch (error) {
          done(error);
        }
      });
      send({
        id: 1,
        method: "initialize",
        params: {
          clientInfo: { name: "power_switch_acceptance", version: "0.1.0" },
          capabilities: { experimentalApi: true },
        },
      });
    });
  } finally {
    child.kill();
  }
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
