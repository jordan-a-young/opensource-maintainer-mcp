import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { fileURLToPath } from "node:url";

// Actual Rust initialize/tools-list only. This does not invoke a GitHub-reading tool.
const client = new Client({ name: "companion-stdio-smoke", version: "0.0.0" });
const transport = new StdioClientTransport({
  command: fileURLToPath(
    new URL(
      `../../target/debug/opensource-maintainer-mcp${process.platform === "win32" ? ".exe" : ""}`,
      import.meta.url,
    ),
  ),
  args: [fileURLToPath(new URL("../../examples/config.toml", import.meta.url))],
  stderr: "ignore",
  maxBufferSize: 4 * 1024 * 1024,
});
try {
  await client.connect(transport, { timeout: 10_000 });
  const { tools } = await client.listTools({}, { timeout: 10_000 });
  const expected = [
    "list_projects",
    "get_project_overview",
    "audit_repository",
    "compare_standards",
    "analyze_ci",
    "review_release_config",
  ];
  if (tools.length !== 6 || expected.some((name) => !tools.some((t) => t.name === name)))
    throw new Error();
  console.log(
    "Rust stdio initialization passed; all six expected tools advertised. No GitHub tool calls made.",
  );
} catch {
  console.error(
    "Rust stdio smoke failed. Build the debug executable and check examples/config.toml.",
  );
  process.exitCode = 1;
} finally {
  await transport.close();
}
