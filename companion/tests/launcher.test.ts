import { expect, it } from "vite-plus/test";
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath } from "node:url";

// Unix process-group cleanup ensures even a failing regression cannot leak a server.
it.skipIf(process.platform === "win32")(
  "closes the Vite server when only its vp launcher is terminated",
  async () => {
    const reservation = createServer();
    await new Promise<void>((resolve) => reservation.listen(0, "127.0.0.1", resolve));
    const address = reservation.address();
    if (!address || typeof address === "string") throw new Error("No port");
    await new Promise<void>((resolve) => reservation.close(() => resolve()));
    const child = spawn(
      process.execPath,
      ["node_modules/vite-plus/bin/vp", "dev", "--port", String(address.port)],
      {
        cwd: fileURLToPath(new URL("../", import.meta.url)),
        env: { ...process.env, COMPANION_FIXTURE: "1" },
        detached: true,
        stdio: "ignore",
      },
    );
    const origin = `http://127.0.0.1:${address.port}`;
    const headers = { "X-Companion-Request": "1" };
    try {
      let session: { csrf: string } | undefined;
      for (let i = 0; i < 70; i++) {
        try {
          session = await (
            await fetch(`${origin}/api/session`, { headers, signal: AbortSignal.timeout(500) })
          ).json();
          break;
        } catch {
          await delay(100);
        }
      }
      expect(session).toBeDefined();
      const connected = await fetch(`${origin}/api/connect`, {
        method: "POST",
        headers: {
          ...headers,
          Origin: origin,
          "Content-Type": "application/json",
          "X-Companion-CSRF": session!.csrf,
        },
        body: "{}",
      });
      expect(connected.status).toBe(200);
      child.kill("SIGTERM");
      await new Promise<void>((resolve) => child.once("exit", () => resolve()));
      let closed = false;
      for (let i = 0; i < 65; i++) {
        try {
          await fetch(`${origin}/api/session`, { headers, signal: AbortSignal.timeout(500) });
        } catch {
          closed = true;
          break;
        }
        await delay(100);
      }
      expect(closed).toBe(true);
    } finally {
      if (child.pid) {
        try {
          process.kill(-child.pid, "SIGTERM");
        } catch {
          /* Already reaped. */
        }
      }
    }
  },
  20_000,
);
