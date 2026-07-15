import { spawn } from "node:child_process";
import process from "node:process";

const command = process.argv[2];
if (!new Set(["dev", "build"]).has(command)) {
  console.error("Usage: node scripts/next.mjs <dev|build>");
  process.exit(2);
}
process.env.NEXT_TELEMETRY_DISABLED = "1";
const executable = process.platform === "win32" ? "next.cmd" : "next";
const child = spawn(executable, [command], { stdio: "inherit", shell: process.platform === "win32" });
child.on("exit", (code, signal) => {
  if (signal) process.kill(process.pid, signal);
  process.exit(code ?? 1);
});
