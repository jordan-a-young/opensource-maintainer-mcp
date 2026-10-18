import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import {
  createServer,
  request as httpRequest,
  type IncomingHttpHeaders,
  type Server,
} from "node:http";
import { fileURLToPath } from "node:url";
import { Bridge, MAX_BODY, TOOL_NAMES } from "../server/bridge";
import { record } from "../src/types";

const fixture = fileURLToPath(new URL("./fixtures/mcp-server.mjs", import.meta.url));
const bridges: Bridge[] = [];
const servers: Server[] = [];
function bridge(scenario = "normal", timeout = 2000) {
  const instance = new Bridge({
    command: process.execPath,
    args: [fixture, scenario],
    mode: "fixture",
    timeout,
  });
  bridges.push(instance);
  return instance;
}
function rawPost(url: string, headers: Record<string, string>) {
  return new Promise<{ status: number | undefined; headers: IncomingHttpHeaders }>(
    (resolve, reject) => {
      const req = httpRequest(url, { method: "POST", headers }, (res) => {
        res.resume();
        res.once("end", () => resolve({ status: res.statusCode, headers: res.headers }));
      });
      req.on("error", reject);
      req.end("{}");
    },
  );
}
afterEach(async () => {
  await Promise.all(bridges.splice(0).map((b) => b.close()));
  await Promise.all(
    servers.splice(0).map(
      (s) =>
        new Promise<void>((resolve) => {
          s.close(() => resolve());
          s.closeAllConnections();
        }),
    ),
  );
});

async function endpoint(b: Bridge) {
  const server = createServer((req, res) =>
    b.middleware(req, res, () => {
      res.writeHead(404);
      res.end();
    }),
  );
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (!address || typeof address === "string") throw new Error("No address");
  const origin = `http://127.0.0.1:${address.port}`;
  const headers = {
    Origin: origin,
    "X-Companion-Request": "1",
    "X-Companion-CSRF": b.csrf,
    "Content-Type": "application/json",
  };
  return {
    origin,
    headers,
    post: (path: string, body: unknown) =>
      fetch(`${origin}/api/${path}`, { method: "POST", headers, body: JSON.stringify(body) }),
  };
}

describe("loopback API trust boundary", () => {
  it("does not spawn on session GET, denies mutation GETs and untrusted Host/Origin/CSRF", async () => {
    const b = bridge();
    const { origin, headers } = await endpoint(b);
    const session = await fetch(`${origin}/api/session`, { headers });
    expect((await session.json()).connected).toBe(false);
    expect((await fetch(`${origin}/api/connect`, { headers })).status).toBe(405);
    const untrustedHeaders: Record<string, string>[] = [
      { Host: "evil.example" },
      { Origin: "https://evil.example" },
      { "Sec-Fetch-Site": "cross-site" },
      { "X-Companion-CSRF": "wrong" },
      { "X-Companion-CSRF": "é".repeat(64) },
      { "X-Companion-Request": "0" },
    ];
    for (const extra of untrustedHeaders) {
      // Node fetch deliberately rewrites Host. Raw HTTP exercises DNS-rebinding headers.
      const response = await rawPost(`${origin}/api/connect`, { ...headers, ...extra });
      expect(response.status, JSON.stringify(extra)).toBe(403);
      expect(response.headers["access-control-allow-origin"]).toBeUndefined();
    }
    expect(
      (
        await fetch(`${origin}/api/connect`, {
          method: "POST",
          headers: {
            "X-Companion-Request": "1",
            "X-Companion-CSRF": b.csrf,
            "Content-Type": "application/json",
          },
          body: "{}",
        })
      ).status,
    ).toBe(403);
    expect(b.status().connected).toBe(false);
  });

  it("rejects malformed, oversized and non-object bodies and disallowed tools", async () => {
    const b = bridge();
    const { origin, headers, post } = await endpoint(b);
    for (const body of ["[1]", "null", "{"])
      expect((await fetch(`${origin}/api/call`, { method: "POST", headers, body })).status).toBe(
        400,
      );
    expect(
      (
        await fetch(`${origin}/api/call`, {
          method: "POST",
          headers: { ...headers, "Content-Type": "text/plain" },
          body: "{}",
        })
      ).status,
    ).toBe(415);
    expect((await post("call", { name: "exec", arguments: {} })).status).toBe(400);
    expect((await post("call", { name: "list_projects", arguments: [] })).status).toBe(400);
    expect(
      (await post("call", { name: "list_projects", arguments: { x: "a".repeat(MAX_BODY) } }))
        .status,
    ).toBe(413);
    expect((await post("call", { name: "list_projects", arguments: {} })).status).toBe(409);
  });

  it("initializes actual MCP, calls only allowed tools and disconnects through the API", async () => {
    const b = bridge();
    const { post } = await endpoint(b);
    const connected = await post("connect", {});
    expect(connected.status).toBe(200);
    expect((await connected.json()).tools.map((t: { name: string }) => t.name).sort()).toEqual(
      [...TOOL_NAMES].sort(),
    );
    const response = await post("call", { name: "list_projects", arguments: {} });
    const value = await response.json();
    expect(value.result.structuredContent.projects).toHaveLength(4);
    expect(value.durationMs).toBeGreaterThanOrEqual(0);
    expect((await post("disconnect", {})).status).toBe(200);
    expect(b.status().connected).toBe(false);
  });
});

describe("MCP lifecycle and failures", () => {
  it("terminates old child processes on reconnect and disconnect", async () => {
    const b = bridge("lifecycle");
    await b.connect();
    const first = await b.call("list_projects", {});
    const pid = record(first.result.structuredContent).fixture_pid as number;
    expect(pid).toBeGreaterThan(0);
    await b.connect();
    expect(() => process.kill(pid, 0)).toThrow();
    const second = await b.call("list_projects", {});
    const secondPid = record(second.result.structuredContent).fixture_pid as number;
    expect(secondPid).not.toBe(pid);
    await b.close();
    expect(() => process.kill(secondPid, 0)).toThrow();
  });

  it("keeps tool errors distinct from transport failures", async () => {
    const b = bridge();
    await b.connect();
    const response = await b.call("audit_repository", { repository: "not-in-scope" });
    expect(response.result.isError).toBe(true);
    expect(b.status().connected).toBe(true);
  });

  it("disconnect cancels reconnect while the old transport is still closing", async () => {
    const b = bridge();
    await b.connect();
    let release!: () => void;
    let entered!: () => void;
    const closing = new Promise<void>((resolve) => {
      entered = resolve;
    });
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    // oxlint-disable-next-line typescript/unbound-method -- Rebound to the real transport with .call below.
    const original = StdioClientTransport.prototype.close;
    const close = vi
      .spyOn(StdioClientTransport.prototype, "close")
      .mockImplementationOnce(async function (this: StdioClientTransport) {
        await original.call(this);
        entered();
        await gate;
      });
    try {
      // Attach a rejection handler immediately; cancellation must not spawn a replacement.
      const reconnect = b.connect().then(
        () => "connected",
        () => "cancelled",
      );
      await closing;
      const disconnect = b.close();
      release();
      await disconnect;
      expect(await reconnect).toBe("cancelled");
      expect(b.status().connected).toBe(false);
      await expect(b.call("list_projects", {})).rejects.toThrow("Connect to the MCP server first");
      // Cancellation must not prevent a subsequent explicit connection.
      await b.connect();
      expect(b.status().connected).toBe(true);
    } finally {
      release();
      close.mockRestore();
    }
  });

  it.each(["exit", "overflow", "large-result", "malformed", "hang"])(
    "sanitizes %s failures and closes the connection",
    async (scenario) => {
      const b = bridge(scenario, 1000);
      await b.connect();
      await expect(b.call("list_projects", {})).rejects.toThrow("MCP request failed");
      expect(b.status().connected).toBe(false);
    },
  );

  it("cancels a pending call on disconnect and rejects simultaneous calls", async () => {
    const b = bridge("hang");
    await b.connect();
    const call = b.call("list_projects", {});
    const rejected = expect(call).rejects.toThrow("MCP request failed");
    await expect(b.call("list_projects", {})).rejects.toThrow("Another request");
    await b.close();
    await rejected;
    expect(b.status().busy).toBe(false);
  });

  it("bounds initialization time and cleans up an unresponsive server", async () => {
    const b = bridge("no-init", 1000);
    await expect(b.connect()).rejects.toThrow("Could not initialize");
    expect(b.status().connected).toBe(false);
    expect(b.status().busy).toBe(false);
  });

  it("sanitizes startup failure paths and never exposes child stderr", async () => {
    const b = new Bridge({
      command: "/secret/path/missing-binary",
      args: ["/secret/config"],
      mode: "local",
    });
    bridges.push(b);
    await expect(b.connect()).rejects.toThrow("Could not initialize");
    expect(b.status().connected).toBe(false);
    const stderr = bridge("stderr");
    expect(JSON.stringify(await stderr.connect())).not.toContain("SECRET_FIXTURE_DIAGNOSTIC");
  });
});
