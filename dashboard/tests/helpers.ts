import { execFileSync, spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
export const binary = resolve(
  process.env.RONCHI_BROWSER_BINARY ?? "../target/debug/ronchi",
);
export function cli(project: string, ...args: string[]) {
  try {
    return JSON.parse(
      execFileSync(binary, ["--project", project, "--json", ...args], {
        encoding: "utf8",
        timeout: 20000,
      }),
    );
  } catch (e) {
    const x = e as { stdout?: unknown; stderr?: unknown };
    throw new Error(
      `CLI ${args.join(" ")}: ${String(x.stdout)} ${String(x.stderr)}`,
    );
  }
}
export function project() {
  return mkdtempSync(join(tmpdir(), "ronchi-solid-"));
}
export function up(path: string, ...args: string[]) {
  return cli(
    path,
    "up",
    "--detach",
    "--rpc-listen",
    "127.0.0.1:0",
    "--control-listen",
    "127.0.0.1:0",
    ...args,
  ).result;
}
export function down(path: string, id: string) {
  cli(path, "--lab-id", id, "down");
}
export function clean(path: string, id?: string) {
  if (id)
    try {
      cli(path, "--lab-id", id, "down", "--force");
    } catch {}
  rmSync(path, { recursive: true, force: true });
}
export async function rpc(
  address: string,
  method: string,
  params: unknown[] = [],
) {
  const r = await fetch(`http://${address}`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  return r.json() as Promise<any>;
}
export async function viewer(path: string, id: string) {
  const child = spawn(
    binary,
    ["--project", path, "--json", "--lab-id", id, "dashboard", "--serve"],
    { stdio: ["ignore", "pipe", "pipe"] },
  );
  const result = await new Promise<any>((resolve, reject) => {
    const timer = setTimeout(
      () => reject(new Error("Viewer readiness timeout")),
      10000,
    );
    let text = "";
    child.stdout.on("data", (chunk) => {
      text += String(chunk);
      if (text.includes("\n")) {
        clearTimeout(timer);
        resolve(JSON.parse(text.split("\n")[0]).result);
      }
    });
    child.on("exit", () => {
      clearTimeout(timer);
      reject(new Error("Viewer exited before ready"));
    });
  });
  return {
    ...result,
    async stop() {
      child.kill("SIGINT");
      await new Promise<void>((resolve) => child.once("exit", () => resolve()));
    },
  };
}
