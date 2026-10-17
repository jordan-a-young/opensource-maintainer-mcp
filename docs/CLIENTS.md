# Client setup: OpenCode, Pi, Codex, and Kiro

Connect this server as a **local stdio MCP server**. The harness launches the binary;
there is no server URL, port, or separate background service to start. These examples
use a source build because npm distribution is not published yet.

Client documentation checked on 2026-10-17. Configuration examples were checked against
upstream documentation; they are not an end-to-end certification of every client/version.

## Prepare the server

From your checkout, using the stable Rust toolchain described in the [README](../README.md#setup-and-run):

```sh
cargo build --release --locked
```

If you do not already have `config.toml`, copy `examples/config.toml` to it (`cp` on
Linux/macOS/WSL; `Copy-Item` in PowerShell). Edit the owner and topic:

```toml
[github]
owner = "availity"
topic = "availity-opensource"
include_archived = false
```

Record the absolute paths to `target/release/opensource-maintainer-mcp` and `config.toml`.
Replace `/absolute/path/opensource-maintainer-mcp` in every example with your checkout
path. On native Windows the executable ends in `.exe`; see [Windows and WSL](#windows-and-wsl).
Keep each argument separate, even when a path contains spaces.

The server's `config.toml` selects GitHub repositories. The client configuration below
selects the executable and passes that TOML path as its only argument. They are separate
files, even though Codex also names its own settings file `config.toml`.
Merge entries into existing client settings rather than replacing the whole file.
Start without a token; [optional authentication](#optional-github-token) is described below.

## OpenCode

Add this to `opencode.json` in the project where you run OpenCode, or to
`~/.config/opencode/opencode.json` for user-wide availability:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "opensource-maintainer": {
      "type": "local",
      "command": [
        "/absolute/path/opensource-maintainer-mcp/target/release/opensource-maintainer-mcp",
        "/absolute/path/opensource-maintainer-mcp/config.toml"
      ],
      "enabled": true
    }
  }
}
```

Restart OpenCode and run `opencode mcp list` from that project to inspect server status.
Then use the [verification prompt](#verify-the-connection). OpenCode puts the executable
and arguments together in `command`; it does not use the `mcpServers` wrapper.
Sources: [MCP configuration](https://opencode.ai/docs/mcp-servers/) and
[configuration locations](https://opencode.ai/docs/config/).

## Pi

These instructions target [Pi's current built-in MCP support](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/mcp.md).
Check `pi mcp --help` first; if unavailable, your version needs different integration
instructions. Do not assume an older extension-based setup uses the same configuration.

Register the server at user scope:

```sh
pi mcp add opensource-maintainer -- /absolute/path/opensource-maintainer-mcp/target/release/opensource-maintainer-mcp /absolute/path/opensource-maintainer-mcp/config.toml
pi mcp list
```

Alternatively, merge this into `~/.pi/agent/mcp.json`, or `.pi/mcp.json` in a trusted project:

```json
{
  "mcpServers": {
    "opensource-maintainer": {
      "command": "/absolute/path/opensource-maintainer-mcp/target/release/opensource-maintainer-mcp",
      "args": ["/absolute/path/opensource-maintainer-mcp/config.toml"]
    }
  }
}
```

In Pi, run `/reload` after external configuration changes, then `/mcp` to inspect the
connection and tools. Use the [verification prompt](#verify-the-connection). Tools may be
exposed through codemode/tool search rather than listed directly in the model's tool set.
An extension that registers `/mcp` replaces built-in session MCP support; resolve that
conflict before relying on this guide.

## Codex

Register the server with the CLI:

```sh
codex mcp add opensource-maintainer -- /absolute/path/opensource-maintainer-mcp/target/release/opensource-maintainer-mcp /absolute/path/opensource-maintainer-mcp/config.toml
codex mcp list
```

Alternatively, add this table to `~/.codex/config.toml`, or `.codex/config.toml` in a
trusted project. If `CODEX_HOME` is customized, use that configuration directory.

```toml
[mcp_servers.opensource-maintainer]
command = "/absolute/path/opensource-maintainer-mcp/target/release/opensource-maintainer-mcp"
args = ["/absolute/path/opensource-maintainer-mcp/config.toml"]
```

Restart the session and inspect `/mcp` in the CLI, then use the
[verification prompt](#verify-the-connection). Listing a configuration alone does not
prove the server connected. Local desktop and IDE clients share MCP configuration on
the same Codex host; ensure you configure the host that will run the binary. This does
not register a local binary with ChatGPT web.
Source: [official OpenAI MCP documentation](https://developers.openai.com/codex/mcp).

## Kiro

Merge this into `.kiro/settings/mcp.json` for the workspace or
`~/.kiro/settings/mcp.json` for your user:

```json
{
  "mcpServers": {
    "opensource-maintainer": {
      "command": "/absolute/path/opensource-maintainer-mcp/target/release/opensource-maintainer-mcp",
      "args": ["/absolute/path/opensource-maintainer-mcp/config.toml"],
      "disabled": false
    }
  }
}
```

In the IDE, search the command palette for MCP to open the workspace/user configuration.
Enable MCP support in settings if needed, save, and inspect the server in the MCP panel.
In the CLI, inspect `/mcp`. Then use the [verification prompt](#verify-the-connection).
If a custom agent overrides this server or excludes MCP JSON configuration, check that
agent's settings. No blanket `autoApprove` list is required by this guide.
Sources: [Kiro configuration](https://kiro.dev/docs/mcp/configuration/) and
[custom-agent configuration](https://kiro.dev/docs/custom-agents/configuration-reference/).

## Windows and WSL

Use paths and binaries for the environment that launches the MCP process:

- **Harness inside WSL:** use the Linux binary and Linux absolute paths, as above.
- **Native Windows harness:** build with native Windows Rust and use the `.exe` plus
  Windows configuration path. JSON paths can use forward slashes, such as
  `C:/projects/opensource-maintainer-mcp/config.toml`, or escaped backslashes.
- **Native Windows harness launching a WSL build:** explicitly launch `wsl.exe`.
  A Windows process cannot directly execute the Linux binary using a `/home/...` path.

For Pi/Kiro, replace the server's `command` and `args` fields with this fragment (inside
its existing server entry); Codex uses the equivalent TOML fields:

```json
{
  "command": "wsl.exe",
  "args": [
    "--distribution", "Ubuntu", "--exec",
    "/home/your-user/projects/opensource-maintainer-mcp/target/release/opensource-maintainer-mcp",
    "/home/your-user/projects/opensource-maintainer-mcp/config.toml"
  ]
}
```

For OpenCode, put `wsl.exe` followed by all those arguments into its single `command`
array. Replace `Ubuntu` with the installed distribution name from `wsl.exe --list --quiet`
and replace both Linux paths. The binary and config must exist inside that distribution.
This direct launch does not load interactive shell startup files; do not assume a token
exported in a WSL terminal reaches a Windows-launched server. Verify anonymous access
first. See [Microsoft's WSL command reference](https://learn.microsoft.com/en-us/windows/wsl/basic-commands/).

## Optional GitHub token

`GITHUB_TOKEN` is optional and needs no write permissions. It improves discovery quota;
repository content reads intentionally stay anonymous. The server does not use `gh`
login credentials automatically. Never put a token in the server TOML or commit it.

If needed, provide the token to the harness environment and explicitly forward it in
its server entry. Add these fields only when that environment variable is available:

| Harness | Field in the server entry |
| --- | --- |
| OpenCode | `"environment": {"GITHUB_TOKEN": "{env:GITHUB_TOKEN}"}` |
| Pi | `"env": {"GITHUB_TOKEN": "${GITHUB_TOKEN}"}` |
| Codex | `env_vars = ["GITHUB_TOKEN"]` |
| Kiro | `"env": {"GITHUB_TOKEN": "${GITHUB_TOKEN}"}` |

These are client-specific syntaxes, documented in the sources above. Kiro may ask you
to approve environment-variable expansion. Desktop processes may not inherit terminal
variables; reconnect after changing their launch environment. For Windows-to-WSL launches,
these fields alone do not guarantee forwarding across the OS boundary.

## Verify the connection

Confirm the harness reports a connected server and six tools (names may have a client
prefix): `list_projects`, `get_project_overview`, `audit_repository`, `compare_standards`,
`analyze_ci`, and `review_release_config`.

Ask:

> Use opensource-maintainer to call list_projects. Show the returned repository names.
> If any match, call get_project_overview for the first name and summarize its setup.

An empty project list is valid: check the configured owner, topic, and archived policy.
Use discovered simple names, not URLs or `owner/name`. Other tool arguments and limits
are in the [tool reference](../README.md#available-tools).

If connection fails, check the executable/config paths and the server's stderr in the
client logs. A process waiting on stdin is normal; it expects an MCP handshake. This
server has no HTTP/OAuth login endpoint. See [troubleshooting](../README.md#troubleshooting)
for configuration errors, rate limits, and skipped inspection results.
