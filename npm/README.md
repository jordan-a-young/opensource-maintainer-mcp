# Open Source Maintainer MCP

A read-only Rust MCP server distributed through npm. Discover and inspect public GitHub
repositories without installing Rust. The Node launcher selects a version-matched native
package; installation never compiles code or runs a postinstall downloader.

These packages are being prepared and are not yet published. Once published:

```sh
npx --yes @jys/opensource-maintainer-mcp@0.1.0 /absolute/path/config.toml
```

Create the configuration yourself (or copy `config.example.toml` from the installed
launcher package):

```toml
[github]
owner = "availity"
topic = "availity-opensource"
include_archived = false
```

An MCP client configuration would use:

```json
{
  "mcpServers": {
    "opensource-maintainer": {
      "command": "npx",
      "args": ["--yes", "@jys/opensource-maintainer-mcp@0.1.0", "/absolute/path/config.toml"]
    }
  }
}
```

On Windows, clients may require `npx.cmd` and an absolute Windows configuration path.
Set `GITHUB_TOKEN` through your client's environment only if needed for API rate limits.
No write permissions are required. Stdout is reserved for MCP messages; diagnostics use
stderr. The tool does not read your GitHub CLI login.

Requires Node 22.14+ and optional dependencies enabled. Supported binaries: Linux x64
glibc (built on Ubuntu 22.04), Windows x64, macOS Intel and Apple Silicon. Alpine/musl and
other architectures are unsupported. The launcher reports missing or mismatched native
packages instead of silently downloading or compiling a fallback. Do not install with
`--omit=optional`; reinstall with `--include=optional` if the native package is missing.

Tools: `list_projects`, `get_project_overview`, `audit_repository`, `compare_standards`,
`analyze_ci`, and `review_release_config`. See the
[repository README](https://github.com/jordan-a-young/opensource-maintainer-mcp#readme)
for scope and tool documentation. MIT licensed.
