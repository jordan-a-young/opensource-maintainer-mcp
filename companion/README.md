# Maintainer companion

A private, local React + TypeScript app with two workspaces:

- **Repository dashboard:** discover and search public projects, select a repository,
  run overview/audit/CI/release reports, and compare 2–5 projects.
- **MCP testing panel:** inspect the server's actual tool schemas, send form or raw
  JSON arguments, and inspect requests, structured results, raw MCP JSON, errors,
  and timings. Session history is held only in browser memory.

This app uses **Vite+ 1.3.0**, installed locally through npm. `vp dev`, `vp check`,
`vp test`, and `vp build` are the Vite+ toolchain, not standalone Vite commands.
It is isolated from the Rust crate and the `npm/` native-server distribution.

## Setup and run

Use **Node 24.18+ (24.x) and npm 11**. No global Vite+ installation is needed.
From the repository root:

```sh
cargo build --locked
```

Create `config.toml` from `examples/config.toml` if you have not already done so,
and configure its owner/topic as described in [server setup](../docs/SETUP.md).
Then:

```sh
cd companion
npm ci
npm run dev
```

Open **http://127.0.0.1:5173** (use this numeric address, not `localhost`). Click
**Connect server**, then **Discover**. Connect performs MCP initialization and
`tools/list`; it does not read GitHub. Selecting/searching projects never runs
tools automatically. Each report requires an explicit button click.

Completion feedback names the requested repositories. **View result** focuses the
report; when you have changed the dashboard selection, it opens that completed call
in the testing panel instead. Calls remain available in session history, and selecting
history never replaces the dashboard's report.

The testing panel preserves argument drafts separately for each tool. Switching to
**Form** synchronizes supported raw arguments. Invalid JSON, extra fields, or values
the form cannot represent keep **Form** disabled with an explanation; the raw draft
is preserved, and valid JSON objects can still be sent for protocol testing.

### Operator configuration

The Node bridge resolves these environment variables once on startup. They are
never browser settings and are not embedded in the frontend:

| Variable            | Default / meaning                                                        |
| ------------------- | ------------------------------------------------------------------------ |
| `COMPANION_BINARY`  | `../target/debug/opensource-maintainer-mcp` (`.exe` on Windows)          |
| `COMPANION_CONFIG`  | `../config.toml`                                                         |
| `GITHUB_TOKEN`      | Optional token inherited by the Rust child only                          |
| `RUST_LOG`          | Optional Rust logging filter; child diagnostics are discarded by this UI |
| `COMPANION_FIXTURE` | Set to exactly `1` to use the bundled offline MCP fixture                |

Relative binary/config paths are resolved against **`companion/`**, regardless of
the shell working directory. Use an absolute path for an externally installed
binary. The command receives exactly one fixed config-path argument; the browser
cannot supply commands, environment variables, binary paths, or config paths.

For a release build, for example (POSIX shell):

```sh
COMPANION_BINARY=../target/release/opensource-maintainer-mcp npm run dev
```

In PowerShell, set `$env:COMPANION_BINARY = '../target/release/opensource-maintainer-mcp.exe'`
before `npm run dev`. Use your shell's environment or secret mechanism for tokens;
do not use a `VITE_`-prefixed token or put credentials in frontend files.

## Offline browser smoke test

```sh
cd companion
npm run dev:fixture
```

This cross-platform command starts the same Vite+ app and same SDK stdio bridge,
but selects the fixed `tests/fixtures/mcp-server.mjs` process. It **does not need
Rust, configuration, credentials, or GitHub access**. The UI displays
`FIXTURE · NO NETWORK`. The fixture receives no GitHub token.

1. Open http://127.0.0.1:5173 and connect.
2. Discover four sample projects, search, and open `element`.
3. Run Overview, Audit, CI, and Release individually.
4. Check `element` and `availity-react`, then compare selected. Inspect different,
   incomplete, absent/unknown, and not-applicable observations.
5. In the testing panel, inspect schemas, send raw JSON, and inspect history.
   An unknown repository name produces a safe MCP tool error; invalid JSON is
   rejected before sending.
6. Disconnect and reconnect, resize to mobile, and check keyboard navigation.

For preview, set `COMPANION_FIXTURE=1` in the operator environment before
`npm run preview`. Fixture mode is selected on server startup, never via a URL or
browser toggle. Stop the fixture server before starting the real-server mode.

## Checks and build

Run from `companion/`:

```sh
npm run check        # Vite+ formatting, Oxlint, type-aware lint and TypeScript checks
npm test             # Vite+/Vitest bridge integration and React presentation tests
npm run build        # Vite+/Rolldown production frontend -> ignored dist/
npm run smoke:stdio  # Real Rust initialize + tools/list; no GitHub calls
npm run preview      # Same local bridge + built frontend, http://127.0.0.1:4173
```

`npm run fmt` applies formatting. The smoke command requires the debug Rust
executable and always uses the sanitized `examples/config.toml`, never your config.
The SDK may print an advisory about Rust's `uint` schema format; normal JSON Schema
numeric constraints still apply.

`npm exec -- vp toolchain` inspects the bundled tools. `npm exec -- vp run <script>`
runs package scripts, whereas `npm exec -- vp <command>` runs built-in toolchain
commands. The npm lockfile pins all dependencies. The `vite` override aliases
`@voidzero-dev/vite-plus-core@1.3.0`; the `vitest` override matches the bundled
5.0.5, confirmed with `npm exec -- vp toolchain vitest`.

## Local bridge and limits

`server/bridge.ts` is a Vite+ plugin installed in **both dev and preview**. A static
hosting service serving only `dist/` cannot provide its API. This is a local
single-operator tool; dev and preview bind only to **127.0.0.1**, refuse remote
host overrides, and do not support reverse proxies or remote deployment.

- Every HTTP request validates the exact loopback Host and any Origin. API requests
  require a custom header; mutations require POST, exact same Origin, JSON content
  type, and a random server-session CSRF token. CORS is disabled. Cross-site
  requests and framing are denied. `GET /api/session` never spawns a child.
- `POST /api/connect`, `/api/disconnect`, and `/api/call` are the only mutations.
  Calls are restricted to the six existing read-only tools. The official MCP SDK
  handles initialize, tool listing, schema validation, and stdio calls.
- At most one child and one tool/connection operation run at a time. Disconnect
  cancels an active operation. Reconnect replaces the child. Transport failures,
  timeouts, response overflow, and server shutdown clean up the child; the SDK
  closes stdin and escalates through TERM/KILL if necessary.
  A lightweight parent-process watcher also closes the server if the Vite+ launcher
  is terminated without forwarding its signal to Vite.
- Requests: **16 KiB**, JSON objects, **5 seconds** to receive a body. Initialization
  and tool listing: **10 seconds each**. Tool calls: **120 seconds**. Stdio buffering:
  **4 MiB**. Browser response payload: **2 MiB**. Exceeding a tool-response limit
  fails explicitly and disconnects; no partial report is presented.
- Browser history: **12 calls / 4 MiB**, newest first. Current report and discovered
  projects are also retained only in memory. Reload clears browser history;
  **Disconnect** closes the shared server process. Multiple tabs share that one
  process, but keep their own history. Closing a tab alone is not a disconnect;
  stop the dev/preview server when done.
- No config contents or tokens are returned, and child stderr is discarded rather
  than reflected. Transport/startup failures are sanitized. Rust tool errors retain
  the server's safe public messages. Reports render untrusted data as React text;
  scripts and URLs are displayed as data, never executed or turned into links.
- Findings, unknowns, absence, and inapplicability are displayed separately. Static
  comparisons are not compatibility judgments; CI and release results are not
  certifications. The original [tool scope and budgets](../docs/TOOLS.md) still apply.

Tests launch the real SDK against the offline fixture and cover Host/Origin/CSRF,
body bounds, tool allowlisting, process lifecycle, errors, cancellation, and UI
status preservation. No automated test reads live GitHub.
