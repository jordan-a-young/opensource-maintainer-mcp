import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

// Fixed, operator-only fixture mode, including on Windows. No browser command input.
const child = spawn(
  process.execPath,
  [
    fileURLToPath(new URL("../node_modules/vite-plus/bin/vp", import.meta.url)),
    "dev",
    ...process.argv.slice(2),
  ],
  {
    stdio: "inherit",
    env: { ...process.env, COMPANION_FIXTURE: "1" },
  },
);
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => child.kill(signal));
child.on("exit", (code) => {
  process.exitCode = code ?? 0;
});
