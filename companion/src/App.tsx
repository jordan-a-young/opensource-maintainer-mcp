import { useEffect, useRef, useState, type RefObject } from "react";
import { Report } from "./Report";
import {
  TOOLS,
  projectList,
  retainHistory,
  type CallResponse,
  type Entry,
  type Project,
  type Session,
  type ToolName,
} from "./types";

const INITIAL: Session = { connected: false, busy: false, mode: "local", tools: [] };

interface Draft {
  raw: string;
  rawMode: boolean;
  repository: string;
  repositories: string;
}

function initialDraft(tool: ToolName): Draft {
  return {
    raw: JSON.stringify(
      tool === "list_projects"
        ? {}
        : tool === "compare_standards"
          ? { repositories: [] }
          : { repository: "" },
      null,
      2,
    ),
    rawMode: false,
    repository: "",
    repositories: "",
  };
}

function parseArguments(raw: string): Record<string, unknown> {
  const args: unknown = JSON.parse(raw);
  if (!args || typeof args !== "object" || Array.isArray(args)) throw new Error();
  return args as Record<string, unknown>;
}

function formIssue(tool: ToolName, raw: string): string {
  let args: Record<string, unknown>;
  try {
    args = parseArguments(raw);
  } catch {
    return "Form unavailable: arguments must be a valid JSON object. Your raw draft is preserved.";
  }
  const keys = Object.keys(args);
  const supported =
    tool === "list_projects"
      ? keys.length === 0
      : tool === "compare_standards"
        ? keys.length === 1 &&
          Array.isArray(args.repositories) &&
          args.repositories.every(
            (name) =>
              typeof name === "string" &&
              name.length > 0 &&
              name === name.trim() &&
              !name.includes(","),
          )
        : keys.length === 1 && typeof args.repository === "string";
  return supported
    ? ""
    : "Form unavailable: this draft contains fields or values the form cannot represent. Edit or send it in Raw JSON; all arguments are preserved.";
}

function requestTarget(entry: Pick<Entry, "name" | "args">): string {
  if (entry.name === "list_projects") return "Configured public project scope";
  const target =
    entry.name === "compare_standards" ? entry.args.repositories : entry.args.repository;
  if (typeof target === "string" && target) return target;
  if (
    Array.isArray(target) &&
    target.length &&
    target.every((name) => typeof name === "string" && name)
  )
    return target.join(", ");
  return "Missing or invalid repository target (see request arguments)";
}

async function request<T>(path: string, csrf?: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api/${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "X-Companion-Request": "1",
      ...(body === undefined
        ? {}
        : { "Content-Type": "application/json", "X-Companion-CSRF": csrf ?? "" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(130_000),
  });
  const value = await response.json();
  if (!response.ok)
    throw new Error(
      typeof value.error === "string"
        ? value.error
        : "The local bridge could not complete the request.",
    );
  return value as T;
}

export default function App() {
  const [session, setSession] = useState<Session>(INITIAL);
  const [page, setPage] = useState<"dashboard" | "developer">("dashboard");
  const [pending, setPending] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [projects, setProjects] = useState<Project[]>([]);
  const [discovered, setDiscovered] = useState(false);
  const [search, setSearch] = useState("");
  const [selected, setSelected] = useState<Project | null>(null);
  const [comparison, setComparison] = useState<string[]>([]);
  const [dashboardEntry, setDashboardEntry] = useState<Entry | null>(null);
  const [developerEntry, setDeveloperEntry] = useState<Entry | null>(null);
  const [completed, setCompleted] = useState<Entry | null>(null);
  const [history, setHistory] = useState<Entry[]>([]);
  const [tool, setTool] = useState<ToolName>("list_projects");
  const [drafts, setDrafts] = useState<Partial<Record<ToolName, Draft>>>({});
  const draft = drafts[tool] ?? initialDraft(tool);
  const { raw, rawMode, repository, repositories } = draft;
  const draftIssue = formIssue(tool, raw);
  function updateDraft(update: Partial<Draft>) {
    setDrafts((current) => ({
      ...current,
      [tool]: { ...(current[tool] ?? initialDraft(tool)), ...update },
    }));
  }
  const dashboardRevision = useRef(0);
  const resultRef = useRef<HTMLElement>(null);
  const completionRef = useRef<HTMLDivElement>(null);
  const [focusRequest, setFocusRequest] = useState(0);
  const nextId = useRef(1);

  useEffect(() => {
    if (focusRequest && resultRef.current) {
      // Keep the result heading below the sticky feedback, including wrapped mobile text.
      resultRef.current.style.scrollMarginTop = `${(completionRef.current?.offsetHeight ?? 0) + 24}px`;
      resultRef.current.focus({ preventScroll: true });
      resultRef.current.scrollIntoView?.({ block: "start" });
    }
  }, [focusRequest]);

  function viewResult() {
    if (!completed) return;
    if (page !== "dashboard" || dashboardEntry?.id !== completed.id) {
      setDeveloperEntry(completed);
      setPage("developer");
    }
    setFocusRequest((current) => current + 1);
  }

  async function refreshSession() {
    const current = await request<Session>("session");
    setSession(current);
    return current;
  }
  useEffect(() => {
    void refreshSession().catch(() =>
      setError(
        "The local bridge is unavailable. Start the companion dev or preview server and reload.",
      ),
    );
  }, []);

  async function connection(connect: boolean) {
    if (connect) setPending("Connecting to local MCP server…");
    setError("");
    try {
      const current = await request<Session>(connect ? "connect" : "disconnect", session.csrf, {});
      setSession((previous) => ({ ...current, csrf: previous.csrf }));
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : "Connection failed.");
    } finally {
      if (connect) setPending(null);
      await refreshSession().catch(() => setSession(INITIAL));
    }
  }

  async function run(name: ToolName, args: Record<string, unknown>) {
    if (pending) return;
    setPending(`Running ${name} — ${requestTarget({ name, args })}…`);
    setCompleted(null);
    setError("");
    const source = page;
    const revision = dashboardRevision.current;
    const started = performance.now();
    const item: Entry = {
      id: nextId.current++,
      name,
      args,
      time: new Date().toLocaleTimeString(),
      durationMs: 0,
    };
    try {
      const response = await request<CallResponse>("call", session.csrf, { name, arguments: args });
      item.response = response.result;
      item.durationMs = response.durationMs;
      if (response.result.isError)
        item.error = "The MCP server returned a tool error. Inspect the response below.";
      if (name === "list_projects" && !response.result.isError) {
        const found = projectList(response.result);
        setProjects(found);
        setDiscovered(true);
        setSelected((current) => found.find((p) => p.name === current?.name) ?? null);
        setComparison((current) => current.filter((name) => found.some((p) => p.name === name)));
      }
    } catch (failure) {
      item.durationMs = Math.round(performance.now() - started);
      item.error =
        failure instanceof Error ? failure.message : "Request failed. Check the local bridge.";
    } finally {
      if (source === "dashboard" && revision === dashboardRevision.current) setDashboardEntry(item);
      if (source === "developer") setDeveloperEntry(item);
      setCompleted(item);
      setHistory((current) => retainHistory([item, ...current]));
      setPending(null);
      await refreshSession().catch(() => setSession(INITIAL));
    }
  }

  function submitTool() {
    try {
      const args = parseArguments(raw);
      if (
        new TextEncoder().encode(JSON.stringify({ name: tool, arguments: args })).byteLength >
        16 * 1024
      ) {
        setError("Arguments exceed the 16 KiB request limit.");
        return;
      }
      void run(tool, args);
    } catch {
      setError("Arguments must be a valid JSON object.");
    }
  }
  const enabled = session.connected && !pending;
  const visible = projects.filter((p) =>
    `${p.full_name} ${p.description ?? ""} ${p.language ?? ""}`
      .toLowerCase()
      .includes(search.toLowerCase()),
  );
  const schema = session.tools.find((s) => s.name === tool);

  return (
    <div className="app-shell">
      <header className="topbar">
        <a className="brand" href="#main">
          <span className="brand-mark" aria-hidden="true">
            m<span>·</span>
          </span>
          <span>
            maintainer<span className="brand-sub">repository companion</span>
          </span>
        </a>
        <div className="connection">
          <span className={`connection-state ${session.connected ? "online" : ""}`}>
            <span aria-hidden="true" />
            {session.connected ? "Connected" : "Disconnected"}
          </span>
          {session.connected || pending ? (
            <button className="button secondary" onClick={() => void connection(false)}>
              Disconnect
            </button>
          ) : (
            <button
              className="button"
              onClick={() => void connection(true)}
              disabled={!session.csrf}
            >
              Connect server
            </button>
          )}
        </div>
      </header>
      <div className="workspace-bar">
        <nav aria-label="Workspace">
          <button
            className={page === "dashboard" ? "nav-tab active" : "nav-tab"}
            aria-current={page === "dashboard" ? "page" : undefined}
            onClick={() => setPage("dashboard")}
          >
            Repository dashboard
          </button>
          <button
            className={page === "developer" ? "nav-tab active" : "nav-tab"}
            aria-current={page === "developer" ? "page" : undefined}
            onClick={() => setPage("developer")}
          >
            MCP testing panel
          </button>
        </nav>
        <span className="local-label">
          {session.mode === "fixture" ? "FIXTURE · NO NETWORK" : "LOCAL · READ ONLY"}
        </span>
      </div>
      <main id="main">
        <div className="page-heading">
          <div>
            <p className="eyebrow">
              {page === "dashboard" ? "PUBLIC REPOSITORY INTELLIGENCE" : "DEVELOPER WORKSPACE"}
            </p>
            <h1>
              {page === "dashboard"
                ? "A clearer view of your projects."
                : "Inspect the protocol. Explore the tools."}
            </h1>
            <p className="lede">
              {page === "dashboard"
                ? "Discover your repositories, inspect the evidence, and compare conventions."
                : "Run explicit MCP calls against your local server, with schemas and session-only history."}
            </p>
          </div>
          <span className="tool-count">
            <strong>06</strong> inspection tools
          </span>
        </div>
        {error && (
          <div className="notice error" role="alert">
            {error}
            <button onClick={() => setError("")} aria-label="Dismiss error">
              ×
            </button>
          </div>
        )}
        <div
          ref={completionRef}
          className={pending || completed ? "notice progress completion" : "completion"}
        >
          <div role="status" aria-live="polite" aria-atomic="true">
            {pending ? (
              <>
                <span className="spinner" aria-hidden="true" /> {pending} Disconnect to cancel.
              </>
            ) : completed ? (
              `${completed.error ? "Failed" : "Completed"}: ${completed.name} — ${requestTarget(completed)}.`
            ) : null}
          </div>
          {!pending && completed && (
            <button className="button secondary" onClick={viewResult}>
              View result
            </button>
          )}
        </div>
        {!session.connected && !pending && (
          <div className="notice">
            <strong>Start with a connection.</strong> Connect initializes the local stdio server and
            lists its tools. Repository reads run only when you request them.
          </div>
        )}
        {page === "dashboard" ? (
          <div className="dashboard-grid">
            <aside className="panel project-panel" aria-label="Project selection">
              <div className="panel-heading">
                <h2>
                  Projects <span className="count">{projects.length}</span>
                </h2>
                <button
                  className="button small"
                  disabled={!enabled}
                  onClick={() => void run("list_projects", {})}
                >
                  {discovered ? "Refresh" : "Discover"}
                </button>
              </div>
              <label className="search-label">
                <span>Search projects</span>
                <input
                  type="search"
                  placeholder="Name, language, description…"
                  value={search}
                  onChange={(e) => setSearch(e.target.value)}
                />
              </label>
              <p className="hint">Open a project to inspect it. Check 2–5 to compare.</p>
              <div className="project-list">
                {visible.map((project) => (
                  <div
                    className={`project-row ${selected?.name === project.name ? "selected" : ""}`}
                    key={project.name}
                  >
                    <input
                      type="checkbox"
                      aria-label={`Compare ${project.name}`}
                      checked={comparison.includes(project.name)}
                      disabled={!comparison.includes(project.name) && comparison.length >= 5}
                      onChange={(e) => {
                        dashboardRevision.current += 1;
                        setDashboardEntry(null);
                        setComparison((current) =>
                          e.target.checked
                            ? [...current, project.name]
                            : current.filter((name) => name !== project.name),
                        );
                      }}
                    />
                    <button
                      className="project-button"
                      aria-label={`Open ${project.name}`}
                      aria-pressed={selected?.name === project.name}
                      onClick={() => {
                        setSelected(project);
                        dashboardRevision.current += 1;
                        setDashboardEntry(null);
                      }}
                    >
                      <strong>{project.name}</strong>
                      <span>
                        {project.language ?? "Language unknown"}
                        {project.archived ? " · Archived" : ""}
                      </span>
                    </button>
                  </div>
                ))}
              </div>
              {visible.length === 0 && (
                <div className="empty compact">
                  <span className="empty-symbol" aria-hidden="true">
                    ⌕
                  </span>
                  <h3>
                    {!discovered
                      ? "Your projects, in one place"
                      : projects.length
                        ? "No search matches"
                        : "No matching public projects"}
                  </h3>
                  <p>
                    {!discovered
                      ? "Connect, then discover the public repositories in your configured scope."
                      : projects.length
                        ? "Try a different name or language."
                        : "Check the operator's owner, topic, and archive policy, then refresh."}
                  </p>
                </div>
              )}
              <div className="compare-footer">
                <span>{comparison.length} of 5 selected</span>
                <button
                  className="button secondary"
                  disabled={!enabled || comparison.length < 2}
                  onClick={() => void run("compare_standards", { repositories: comparison })}
                >
                  Compare selected
                </button>
                {comparison.length > 0 && (
                  <button
                    className="text-button"
                    onClick={() => {
                      setComparison([]);
                      dashboardRevision.current += 1;
                      setDashboardEntry(null);
                    }}
                  >
                    Clear selection
                  </button>
                )}
              </div>
            </aside>
            <div className="report-column">
              {selected ? (
                <section className="panel selected-project">
                  <p className="eyebrow">SELECTED REPOSITORY</p>
                  <h2>{selected.full_name}</h2>
                  <p>{selected.description ?? "No description provided."}</p>
                  <div className="project-meta">
                    <span>{selected.language ?? "Language unknown"}</span>
                    <span>Branch: {selected.default_branch}</span>
                    <span>{selected.archived ? "Archived" : "Active"}</span>
                  </div>
                  <div className="tool-actions">
                    {TOOLS.filter(
                      (t) => !["list_projects", "compare_standards"].includes(t.name),
                    ).map((t) => (
                      <button
                        className="button secondary"
                        disabled={!enabled}
                        key={t.name}
                        onClick={() => void run(t.name, { repository: selected.name })}
                      >
                        {t.short}
                      </button>
                    ))}
                  </div>
                </section>
              ) : (
                <section className="panel empty">
                  <span className="empty-symbol" aria-hidden="true">
                    ▥
                  </span>
                  <h2>From repository to understanding.</h2>
                  <p>
                    Select a discovered project to explore its overview, maintenance checks, CI, or
                    release configuration. Every report keeps its evidence and limitations.
                  </p>
                  <div className="feature-labels">
                    <span>Root manifests</span>
                    <span>Static workflow checks</span>
                    <span>Literal comparisons</span>
                  </div>
                </section>
              )}
              {dashboardEntry ? (
                <Result entry={dashboardEntry} resultRef={resultRef} />
              ) : (
                <div className="scope-note">
                  <strong>Evidence, not a score.</strong>
                  <p>
                    Unknown and not-applicable states remain distinct. No repository scripts are
                    executed, and no security or publication certification is implied.
                  </p>
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="developer-grid">
            <section className="panel request-panel">
              <div className="panel-heading">
                <h2>Tool request</h2>
                <span className="protocol">MCP / stdio</span>
              </div>
              <label>
                Tool
                <select
                  value={tool}
                  onChange={(e) => {
                    setTool(e.target.value as ToolName);
                  }}
                >
                  {TOOLS.map((t) => (
                    <option key={t.name} value={t.name}>
                      {t.name}
                    </option>
                  ))}
                </select>
              </label>
              <p className="hint">
                {schema?.description ?? TOOLS.find((t) => t.name === tool)?.description}
              </p>
              <div className="segmented" aria-label="Argument editor">
                <button
                  aria-pressed={!rawMode}
                  disabled={Boolean(draftIssue)}
                  aria-describedby={draftIssue ? "form-issue" : undefined}
                  onClick={() => {
                    const args = parseArguments(raw);
                    updateDraft({
                      rawMode: false,
                      repository: typeof args.repository === "string" ? args.repository : "",
                      repositories: Array.isArray(args.repositories)
                        ? args.repositories.join(", ")
                        : "",
                    });
                  }}
                >
                  Form
                </button>
                <button
                  aria-pressed={rawMode}
                  onClick={() => {
                    updateDraft({ rawMode: true });
                  }}
                >
                  Raw JSON
                </button>
              </div>
              {rawMode && draftIssue && (
                <p id="form-issue" className="hint">
                  {draftIssue}
                </p>
              )}
              {rawMode ? (
                <label>
                  Arguments
                  <textarea
                    spellCheck={false}
                    value={raw}
                    onChange={(e) => updateDraft({ raw: e.target.value })}
                    rows={9}
                    maxLength={16 * 1024}
                  />
                </label>
              ) : tool === "list_projects" ? (
                <div className="no-args">
                  <code>{"{}"}</code>
                  <p>This tool needs no arguments.</p>
                </div>
              ) : tool === "compare_standards" ? (
                <label>
                  Repository names (2–5)
                  <input
                    value={repositories}
                    onChange={(e) =>
                      updateDraft({
                        repositories: e.target.value,
                        raw: JSON.stringify(
                          {
                            repositories: e.target.value
                              .split(",")
                              .map((name) => name.trim())
                              .filter(Boolean),
                          },
                          null,
                          2,
                        ),
                      })
                    }
                    placeholder="element, availity-react"
                    maxLength={1000}
                  />
                  <span className="hint">
                    Comma-separated simple names, without owner prefixes.
                  </span>
                </label>
              ) : (
                <label>
                  Repository name
                  <input
                    value={repository}
                    onChange={(e) =>
                      updateDraft({
                        repository: e.target.value,
                        raw: JSON.stringify({ repository: e.target.value }, null, 2),
                      })
                    }
                    placeholder="element"
                    maxLength={100}
                  />
                  <span className="hint">Use a simple name returned by list_projects.</span>
                </label>
              )}
              <button className="button run-button" disabled={!enabled} onClick={submitTool}>
                Run tool <span aria-hidden="true">→</span>
              </button>
              <details className="raw">
                <summary>Server-advertised schemas</summary>
                {schema ? (
                  <>
                    <h3>Input</h3>
                    <pre>{JSON.stringify(schema.inputSchema, null, 2)}</pre>
                    <h3>Output</h3>
                    <pre>
                      {JSON.stringify(
                        schema.outputSchema ?? "No output schema advertised",
                        null,
                        2,
                      )}
                    </pre>
                  </>
                ) : (
                  <p>Connect to load the actual MCP tool schemas.</p>
                )}
              </details>
              <div className="history-heading">
                <h3>Session history</h3>
                <button
                  className="text-button"
                  onClick={() => {
                    setHistory([]);
                    setDeveloperEntry(null);
                  }}
                  disabled={!history.length}
                >
                  Clear
                </button>
              </div>
              <p className="hint">Up to 12 calls / 4 MiB. Memory only; cleared on reload.</p>
              {history.length ? (
                <ol className="history">
                  {history.map((item) => (
                    <li key={item.id}>
                      <button
                        aria-pressed={developerEntry?.id === item.id}
                        aria-label={`${item.error ? "Failed" : "Completed"} ${item.name} ${requestTarget(item)} · ${item.time} · ${item.durationMs} ms`}
                        onClick={() => setDeveloperEntry(item)}
                      >
                        <span
                          className={item.error ? "history-error" : "history-success"}
                          aria-hidden="true"
                        >
                          {item.error ? "!" : "✓"}
                        </span>
                        <span>
                          <strong>{item.name}</strong>
                          <small>{requestTarget(item)}</small>
                          <small>
                            {item.time} · {item.durationMs} ms
                          </small>
                        </span>
                      </button>
                    </li>
                  ))}
                </ol>
              ) : (
                <p className="muted">Your calls will appear here.</p>
              )}
            </section>
            <div className="report-column">
              {developerEntry ? (
                <Result entry={developerEntry} developer resultRef={resultRef} />
              ) : (
                <section className="panel empty">
                  <span className="empty-symbol" aria-hidden="true">
                    {"{ }"}
                  </span>
                  <h2>Your next request starts here.</h2>
                  <p>
                    Choose a tool, inspect its schema, and send arguments. Responses, errors, and
                    timing appear here.
                  </p>
                </section>
              )}
            </div>
          </div>
        )}
      </main>
      <footer>
        <span>Maintainer companion</span>
        <span>Vite+ · React · MCP SDK</span>
        <span>Public repositories. Explicit requests. Local session.</span>
      </footer>
    </div>
  );
}

function Result({
  entry,
  developer = false,
  resultRef,
}: {
  entry: Entry;
  developer?: boolean;
  resultRef: RefObject<HTMLElement | null>;
}) {
  return (
    <section className="panel result-panel" aria-label="Tool result" tabIndex={-1} ref={resultRef}>
      <div className="panel-heading">
        <div>
          <p className="eyebrow">{entry.error ? "REQUEST ERROR" : "INSPECTION RESULT"}</p>
          <h2>{TOOLS.find((t) => t.name === entry.name)?.label}</h2>
        </div>
        <span className="timing">
          {entry.durationMs} ms · {entry.time}
        </span>
      </div>
      <p className="result-target">
        <strong>Request target:</strong> {requestTarget(entry)}
      </p>
      <details open={developer} className="raw">
        <summary>Request arguments</summary>
        <pre>{JSON.stringify(entry.args, null, 2)}</pre>
      </details>
      {entry.error && !entry.response?.isError && <div className="notice error">{entry.error}</div>}
      {entry.response && <Report result={entry.response} />}
    </section>
  );
}
