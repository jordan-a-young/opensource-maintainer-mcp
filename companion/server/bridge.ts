import { randomBytes, timingSafeEqual } from "node:crypto";
import type { IncomingMessage, ServerResponse } from "node:http";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import type { Tool } from "@modelcontextprotocol/sdk/types.js";
import type { Plugin } from "vite-plus";

export const TOOL_NAMES = [
  "list_projects",
  "get_project_overview",
  "audit_repository",
  "compare_standards",
  "analyze_ci",
  "review_release_config",
] as const;
export const MAX_BODY = 16 * 1024;
export const MAX_RESPONSE = 2 * 1024 * 1024;
const APP_ROOT = fileURLToPath(new URL("../", import.meta.url));

export interface BridgeOptions {
  command: string;
  args: string[];
  env?: Record<string, string>;
  mode: "local" | "fixture";
  timeout?: number;
}

// Operator configuration is resolved once, never from a browser request.
export function operatorOptions(): BridgeOptions {
  const fixture = process.env.COMPANION_FIXTURE === "1";
  return {
    command: fixture
      ? process.execPath
      : resolve(
          APP_ROOT,
          process.env.COMPANION_BINARY ||
            `../target/debug/opensource-maintainer-mcp${process.platform === "win32" ? ".exe" : ""}`,
        ),
    args: [
      fixture
        ? resolve(APP_ROOT, "tests/fixtures/mcp-server.mjs")
        : resolve(APP_ROOT, process.env.COMPANION_CONFIG || "../config.toml"),
    ],
    env: fixture
      ? {}
      : Object.fromEntries(
          ["GITHUB_TOKEN", "RUST_LOG"].flatMap((key) =>
            process.env[key] ? [[key, process.env[key]!]] : [],
          ),
        ),
    mode: fixture ? "fixture" : "local",
  };
}

class PublicError extends Error {
  readonly status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export class Bridge {
  readonly csrf = randomBytes(32).toString("hex");
  private client?: Client;
  private transport?: StdioClientTransport;
  private tools: Tool[] = [];
  private busy = false;
  private ready = false;
  private closing: Promise<void> = Promise.resolve();
  private connectionGeneration = 0;

  private readonly options: BridgeOptions;
  constructor(options: BridgeOptions) {
    this.options = options;
  }

  status() {
    return { connected: this.ready, busy: this.busy, mode: this.options.mode, tools: this.tools };
  }

  async close() {
    // Invalidate attempts that are still awaiting previous-process cleanup.
    this.connectionGeneration++;
    await this.closeTransport();
  }

  private async closeTransport() {
    const transport = this.transport;
    this.transport = undefined;
    this.client = undefined;
    this.ready = false;
    this.tools = [];
    // SDK closes stdin, then escalates to TERM/KILL for an unresponsive child.
    const previous = this.closing;
    this.closing = (async () => {
      await previous;
      await transport?.close().catch(() => {});
    })();
    await this.closing;
  }

  private async exclusive<T>(operation: () => Promise<T>): Promise<T> {
    if (this.busy)
      throw new PublicError(409, "Another request is running. Wait or disconnect to cancel it.");
    this.busy = true;
    try {
      return await operation();
    } finally {
      this.busy = false;
    }
  }

  async connect() {
    return this.exclusive(async () => {
      const generation = ++this.connectionGeneration;
      await this.closeTransport();
      if (generation !== this.connectionGeneration)
        throw new PublicError(409, "Connection cancelled. Connect again when ready.");
      const client = new Client({ name: "maintainer-companion", version: "0.0.0" });
      const transport = new StdioClientTransport({
        command: this.options.command,
        args: this.options.args,
        env: this.options.env,
        stderr: "ignore",
        maxBufferSize: 4 * 1024 * 1024,
      });
      this.client = client;
      this.transport = transport;
      const invalidate = () => {
        if (this.client === client) void this.close();
      };
      client.onclose = invalidate;
      client.onerror = invalidate;
      try {
        await client.connect(transport, { timeout: this.options.timeout ?? 10_000 });
        const result = await client.listTools({}, { timeout: this.options.timeout ?? 10_000 });
        if (this.client !== client) throw new Error("Disconnected");
        this.tools = result.tools.filter((tool) => TOOL_NAMES.some((name) => name === tool.name));
        if (
          new Set(this.tools.map((tool) => tool.name)).size !== TOOL_NAMES.length ||
          result.nextCursor
        )
          throw new Error("Unexpected tool catalog");
        if (Buffer.byteLength(JSON.stringify(this.tools)) > MAX_RESPONSE)
          throw new Error("Catalog too large");
        this.ready = true;
        return this.status();
      } catch {
        await this.close();
        throw new PublicError(
          502,
          "Could not initialize the MCP server. Check the operator's binary/config paths and build the Rust executable, then reconnect.",
        );
      }
    });
  }

  async call(name: unknown, args: unknown) {
    if (typeof name !== "string" || !TOOL_NAMES.some((tool) => tool === name)) {
      throw new PublicError(400, "Choose one of the six supported tools.");
    }
    if (!args || typeof args !== "object" || Array.isArray(args))
      throw new PublicError(400, "Arguments must be a JSON object.");
    return this.exclusive(async () => {
      const client = this.client;
      if (!client || !this.ready) throw new PublicError(409, "Connect to the MCP server first.");
      const start = performance.now();
      try {
        const result = await client.callTool(
          { name, arguments: args as Record<string, unknown> },
          undefined,
          { timeout: this.options.timeout ?? 120_000, resetTimeoutOnProgress: false },
        );
        if (this.client !== client) throw new Error("Disconnected");
        const response = { result, durationMs: Math.round(performance.now() - start) };
        if (Buffer.byteLength(JSON.stringify(response)) > MAX_RESPONSE)
          throw new Error("Response too large");
        return response;
      } catch {
        await this.close();
        throw new PublicError(
          502,
          "MCP request failed, timed out, or exceeded the response limit. The process was disconnected; reconnect to try again.",
        );
      }
    });
  }

  private trusted(req: IncomingMessage) {
    const host = `127.0.0.1:${req.socket.localPort}`;
    if (
      req.headers.host !== host ||
      (req.headers.origin && req.headers.origin !== `http://${host}`)
    )
      return false;
    if (req.headers["sec-fetch-site"] === "cross-site") return false;
    return true;
  }

  middleware = (req: IncomingMessage, res: ServerResponse, next: () => void) => {
    res.setHeader("X-Content-Type-Options", "nosniff");
    res.setHeader("X-Frame-Options", "DENY");
    res.setHeader("Referrer-Policy", "no-referrer");
    if (!this.trusted(req)) {
      this.reply(res, 403, { error: "Only the loopback companion origin is allowed." });
      return;
    }
    if (!req.url?.startsWith("/api/")) {
      next();
      return;
    }
    res.setHeader("Cache-Control", "no-store");
    void this.handle(req, res).catch((error: unknown) => {
      this.reply(res, error instanceof PublicError ? error.status : 500, {
        error:
          error instanceof PublicError
            ? error.message
            : "The local bridge could not complete the request.",
      });
    });
  };

  private reply(res: ServerResponse, code: number, value: unknown) {
    if (res.destroyed || res.writableEnded) return;
    res.writeHead(code, { "Content-Type": "application/json; charset=utf-8" });
    res.end(JSON.stringify(value));
  }

  private async handle(req: IncomingMessage, res: ServerResponse) {
    if (req.headers["x-companion-request"] !== "1")
      throw new PublicError(403, "Companion request header required.");
    if (req.method === "GET" && req.url === "/api/session") {
      this.reply(res, 200, { ...this.status(), csrf: this.csrf });
      return;
    }
    if (!["/api/connect", "/api/disconnect", "/api/call"].includes(req.url!))
      throw new PublicError(404, "Unknown bridge endpoint.");
    if (req.method !== "POST") throw new PublicError(405, "Use POST for this operation.");
    if (req.headers.origin !== `http://127.0.0.1:${req.socket.localPort}`)
      throw new PublicError(403, "Same-origin request required.");
    const token = req.headers["x-companion-csrf"];
    if (
      typeof token !== "string" ||
      Buffer.byteLength(token) !== this.csrf.length ||
      !timingSafeEqual(Buffer.from(token), Buffer.from(this.csrf))
    ) {
      throw new PublicError(403, "Session token is invalid. Reload the companion.");
    }
    if (req.headers["content-type"] !== "application/json")
      throw new PublicError(415, "Use application/json.");
    const body = await readBody(req);
    if (req.url === "/api/connect") this.reply(res, 200, await this.connect());
    else if (req.url === "/api/disconnect") {
      await this.close();
      this.reply(res, 200, this.status());
    } else this.reply(res, 200, await this.call(body.name, body.arguments));
  }
}

async function readBody(req: IncomingMessage): Promise<Record<string, unknown>> {
  return new Promise((resolveBody, reject) => {
    let size = 0;
    const chunks: Buffer[] = [];
    const timer = setTimeout(() => fail(new PublicError(408, "Request body timed out.")), 5_000);
    const clean = () => {
      clearTimeout(timer);
      req.off("data", data);
      req.off("end", end);
      req.off("aborted", aborted);
      req.off("error", aborted);
    };
    const fail = (error: PublicError) => {
      clean();
      req.resume();
      reject(error);
    };
    const data = (chunk: Buffer) => {
      size += chunk.length;
      if (size > MAX_BODY) fail(new PublicError(413, "Arguments exceed the 16 KiB request limit."));
      else chunks.push(chunk);
    };
    const aborted = () => fail(new PublicError(400, "Request was interrupted."));
    const end = () => {
      clean();
      try {
        const value: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
        if (!value || typeof value !== "object" || Array.isArray(value))
          throw new Error("Object required");
        resolveBody(value as Record<string, unknown>);
      } catch {
        reject(new PublicError(400, "Request must be a valid JSON object."));
      }
    };
    req.on("data", data);
    req.on("end", end);
    req.on("aborted", aborted);
    req.on("error", aborted);
  });
}

export function companionBridge(): Plugin {
  let bridge: Bridge | undefined;
  let removeLifecycleHandlers = () => {};
  const handleLifecycle = (closeServer: () => Promise<void>) => {
    let stopping = false;
    const stop = (code: number) => {
      if (stopping) return;
      stopping = true;
      void (async () => {
        await bridge?.close();
        await closeServer();
        process.exit(code);
      })();
    };
    const interrupt = () => stop(130);
    // The local vp launcher runs Vite in a child process. Killing only the launcher
    // does not forward its signal; close the bridge if that parent disappears.
    const parentPid = process.ppid;
    const parentWatch = setInterval(() => {
      if (process.ppid !== parentPid) {
        stop(0);
        return;
      }
      try {
        process.kill(parentPid, 0);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ESRCH") stop(0);
      }
    }, 1_000).unref();
    process.once("SIGINT", interrupt);
    removeLifecycleHandlers = () => {
      clearInterval(parentWatch);
      process.off("SIGINT", interrupt);
    };
  };
  return {
    name: "maintainer-loopback-bridge",
    configResolved(config) {
      if (config.server.host !== "127.0.0.1" || config.preview.host !== "127.0.0.1") {
        throw new Error("Companion must bind to 127.0.0.1. Remote hosts are not supported.");
      }
    },
    configureServer(server) {
      bridge = new Bridge(operatorOptions());
      server.middlewares.use(bridge.middleware);
      handleLifecycle(() => server.close());
      server.httpServer?.once("close", () => {
        void bridge?.close();
      });
    },
    configurePreviewServer(server) {
      bridge = new Bridge(operatorOptions());
      server.middlewares.use(bridge.middleware);
      handleLifecycle(() => server.close());
      server.httpServer.once("close", () => {
        void bridge?.close();
      });
    },
    async closeBundle() {
      removeLifecycleHandlers();
      await bridge?.close();
    },
    async closePreviewServer() {
      removeLifecycleHandlers();
      await bridge?.close();
    },
  };
}
