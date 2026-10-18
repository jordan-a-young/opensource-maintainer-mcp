// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vite-plus/test";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import App from "../src/App";
import { Report } from "../src/Report";
import { retainHistory, type CallResponse, type Entry } from "../src/types";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("evidence-preserving report presentation", () => {
  it.each(["workflows", "release_jobs"])(
    "counts all %s checks, including repeated IDs in distinct scopes",
    (collection) => {
      render(
        <Report
          result={{
            structuredContent: {
              checks: [{ id: "root", status: "passed", summary: "Root observation" }],
              [collection]: [
                {
                  path: "one.yml",
                  checks: [
                    { id: "shared", status: "finding", summary: "First scoped finding" },
                    { id: "unknown", status: "not_assessed" },
                  ],
                },
                {
                  path: "two.yml",
                  checks: [
                    { id: "shared", status: "finding", summary: "Second scoped finding" },
                    { id: "na", status: "not_applicable" },
                  ],
                },
              ],
            },
          }}
        />,
      );
      const totals = within(screen.getByRole("region", { name: "Check status counts" }));
      for (const [status, count] of [
        ["passed", "1"],
        ["finding", "2"],
        ["not assessed", "1"],
        ["not applicable", "1"],
      ]) {
        expect(totals.getByText(status).parentElement?.querySelector("strong")?.textContent).toBe(
          count,
        );
      }
      expect(screen.getByText("First scoped finding")).toBeTruthy();
      expect(screen.getByText("Second scoped finding")).toBeTruthy();
      expect(
        totals.getByText(/Totals include root\/discovery, workflow, and release-job checks/),
      ).toBeTruthy();
    },
  );

  it("preserves findings, unknown and inapplicable checks without turning untrusted text into HTML", () => {
    render(
      <Report
        result={{
          structuredContent: {
            scope: "Bounded inspection only",
            checks: [
              {
                id: "a",
                status: "finding",
                summary: "Observed gap",
                evidence: ["package.json#/scripts"],
                recommendation: "Review tooling",
              },
              { id: "b", status: "not_assessed", summary: "Unknown test execution", evidence: [] },
              {
                id: "c",
                status: "not_applicable",
                summary: "No applicable ecosystem",
                evidence: [],
              },
              {
                id: "d",
                status: "passed",
                summary: '<img src=x onerror="alert(1)">',
                evidence: [],
              },
            ],
          },
        }}
      />,
    );
    expect(screen.getAllByText("not assessed").length).toBeGreaterThan(0);
    expect(screen.getAllByText("not applicable").length).toBeGreaterThan(0);
    expect(screen.getByText("Unknown test execution")).toBeTruthy();
    expect(screen.getByText("package.json#/scripts")).toBeTruthy();
    expect(document.querySelector("img")).toBe(null);
    expect(screen.getByText('<img src=x onerror="alert(1)">')).toBeTruthy();
    expect(screen.getByText("Raw MCP result")).toBeTruthy();
  });

  it("retains absent and unknown comparison observations and release limitations", () => {
    render(
      <Report
        result={{
          structuredContent: {
            projects: [{ full_name: "owner/a" }, { full_name: "owner/b" }],
            rows: [
              {
                id: "npm.react",
                status: "incomplete",
                observations: [
                  { repository: "owner/a", status: "absent", values: [], evidence: [] },
                  { repository: "owner/b", status: "not_assessed", values: [], evidence: [] },
                ],
              },
            ],
            package: { provenance: null, private: false, status: "not_assessed" },
            release_jobs: [],
          },
        }}
      />,
    );
    expect(screen.getByText("absent")).toBeTruthy();
    expect(screen.getByText("not assessed")).toBeTruthy();
    expect(screen.getByText("Unknown / not provided")).toBeTruthy();
    expect(screen.getByText(/OIDC permission does not establish/)).toBeTruthy();
  });

  it("bounds history by count and encoded size", () => {
    const base: Entry = { id: 1, name: "list_projects", args: {}, durationMs: 1, time: "now" };
    expect(retainHistory(Array.from({ length: 20 }, (_, id) => ({ ...base, id })))).toHaveLength(
      12,
    );
    expect(
      retainHistory([base, { ...base, response: { content: "é".repeat(3 * 1024 * 1024) } }]),
    ).toHaveLength(1);
  });
});

const sampleProjects = ["alpha", "beta"].map((name) => ({
  name,
  full_name: `example/${name}`,
  description: "Demo",
  language: "TypeScript",
  archived: false,
  default_branch: "main",
}));
const success: CallResponse = {
  durationMs: 4,
  result: { structuredContent: { scope: "Finished inspection" }, content: [] },
};
type Call = { name: string; arguments: Record<string, unknown> };

async function startApp(onCall: (call: Call) => Promise<CallResponse> = async () => success) {
  const calls: Call[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, options?: RequestInit) => {
      if (url === "/api/call") {
        if (typeof options?.body !== "string") throw new Error("Expected a JSON request body");
        const call = JSON.parse(options.body) as Call;
        calls.push(call);
        const value =
          call.name === "list_projects"
            ? { durationMs: 1, result: { structuredContent: { projects: sampleProjects } } }
            : await onCall(call);
        return { ok: true, json: async () => value };
      }
      return {
        ok: true,
        json: async () => ({
          connected: true,
          busy: false,
          mode: "fixture",
          csrf: "test-token",
          tools: [],
        }),
      };
    }),
  );
  render(<App />);
  await screen.findByText("Connected");
  return calls;
}

async function discover() {
  fireEvent.click(screen.getByRole("button", { name: "Discover" }));
  await screen.findByRole("button", { name: "Open alpha" });
}

function openDeveloper(tool = "audit_repository") {
  fireEvent.click(screen.getByRole("button", { name: "MCP testing panel" }));
  fireEvent.change(screen.getByLabelText("Tool"), { target: { value: tool } });
}

describe("request context and accessible completion", () => {
  it("keeps a deferred alpha result out of beta's dashboard and retains it in history", async () => {
    let finish!: (response: CallResponse) => void;
    const deferred = new Promise<CallResponse>((resolve) => {
      finish = resolve;
    });
    const calls = await startApp(async () => deferred);
    const status = screen.getByRole("status");
    expect(status.getAttribute("aria-live")).toBe("polite");
    await discover();
    fireEvent.click(screen.getByRole("button", { name: "Open alpha" }));
    fireEvent.click(screen.getByRole("button", { name: "Audit" }));
    expect(status.textContent).toContain("Running audit_repository — alpha");
    const beta = screen.getByRole("button", { name: "Open beta" });
    fireEvent.click(beta);
    beta.focus();
    await act(async () => {
      finish(success);
    });
    expect(screen.getByRole("status")).toBe(status);
    expect(status.textContent).toContain("Completed: audit_repository — alpha");
    expect(screen.queryByRole("region", { name: "Tool result" })).toBeNull();
    expect(screen.getByRole("heading", { name: "example/beta" })).toBeTruthy();
    expect(document.activeElement).toBe(beta);
    expect(calls.at(-1)?.arguments).toEqual({ repository: "alpha" });

    fireEvent.click(screen.getByRole("button", { name: "View result" }));
    const result = screen.getByRole("region", { name: "Tool result" });
    expect(result.textContent).toContain("Request target: alpha");
    expect(document.activeElement).toBe(result);
    expect(screen.getByRole("button", { name: /Completed audit_repository alpha/ })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Repository dashboard" }));
    expect(screen.queryByRole("region", { name: "Tool result" })).toBeNull();
    expect(screen.getByRole("heading", { name: "example/beta" })).toBeTruthy();
  });

  it("keeps developer history selection separate from a completed dashboard report", async () => {
    await startApp();
    await discover();
    fireEvent.click(screen.getByRole("button", { name: "Open beta" }));
    fireEvent.click(screen.getByRole("button", { name: "Audit" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain(
        "Completed: audit_repository — beta",
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: "View result" }));
    expect(document.activeElement).toBe(screen.getByRole("region", { name: "Tool result" }));
    expect(
      screen.getByRole("button", { name: "Repository dashboard" }).getAttribute("aria-current"),
    ).toBe("page");
    openDeveloper("get_project_overview");
    fireEvent.change(screen.getByLabelText(/Repository name/), { target: { value: "alpha" } });
    fireEvent.click(screen.getByRole("button", { name: /Run tool/ }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain(
        "Completed: get_project_overview — alpha",
      ),
    );
    fireEvent.click(screen.getByRole("button", { name: /Completed list_projects/ }));
    expect(screen.getByRole("region", { name: "Tool result" }).textContent).toContain(
      "Configured public project scope",
    );
    fireEvent.click(screen.getByRole("button", { name: /Completed get_project_overview alpha/ }));
    expect(screen.getByRole("region", { name: "Tool result" }).textContent).toContain(
      "Request target: alpha",
    );
    fireEvent.click(screen.getByRole("button", { name: "Repository dashboard" }));
    expect(screen.getByRole("region", { name: "Tool result" }).textContent).toContain(
      "Request target: beta",
    );
  });

  it.each(["tool", "transport"])(
    "announces %s errors once with a visible target and focusable result",
    async (kind) => {
      await startApp(async () => {
        if (kind === "transport") throw new Error("Local bridge request failed.");
        return {
          durationMs: 1,
          result: {
            isError: true,
            content: [{ type: "text", text: "Repository is outside the configured scope." }],
          },
        };
      });
      openDeveloper();
      fireEvent.change(screen.getByLabelText(/Repository name/), {
        target: { value: "missing-project" },
      });
      fireEvent.click(screen.getByRole("button", { name: /Run tool/ }));
      await waitFor(() =>
        expect(screen.getByRole("status").textContent).toContain(
          "Failed: audit_repository — missing-project",
        ),
      );
      expect(screen.queryAllByRole("alert")).toHaveLength(0);
      fireEvent.click(screen.getByRole("button", { name: "View result" }));
      const result = screen.getByRole("region", { name: "Tool result" });
      expect(result.textContent).toContain("Request target: missing-project");
      expect(result.textContent).toContain(
        kind === "tool"
          ? "Repository is outside the configured scope."
          : "Local bridge request failed.",
      );
      expect(document.activeElement).toBe(result);
    },
  );

  it("shows every comparison target in completion feedback and the dashboard result", async () => {
    await startApp();
    await discover();
    fireEvent.click(screen.getByRole("checkbox", { name: "Compare alpha" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Compare beta" }));
    fireEvent.click(screen.getByRole("button", { name: "Compare selected" }));
    await waitFor(() =>
      expect(screen.getByRole("status").textContent).toContain(
        "Completed: compare_standards — alpha, beta",
      ),
    );
    expect(screen.getByRole("region", { name: "Tool result" }).textContent).toContain(
      "Request target: alpha, beta",
    );
  });
});

describe("lossless argument drafts", () => {
  it("roundtrips valid raw edits through Form and preserves repeated Raw JSON clicks", async () => {
    const calls = await startApp();
    openDeveloper();
    fireEvent.change(screen.getByLabelText(/Repository name/), { target: { value: "alpha" } });
    fireEvent.click(screen.getByRole("button", { name: "Raw JSON" }));
    const raw = '{ "repository": "beta" }';
    fireEvent.change(screen.getByLabelText("Arguments"), { target: { value: raw } });
    fireEvent.click(screen.getByRole("button", { name: "Raw JSON" }));
    expect((screen.getByLabelText("Arguments") as HTMLTextAreaElement).value).toBe(raw);
    fireEvent.click(screen.getByRole("button", { name: "Form" }));
    expect((screen.getByLabelText(/Repository name/) as HTMLInputElement).value).toBe("beta");
    fireEvent.click(screen.getByRole("button", { name: "Raw JSON" }));
    expect((screen.getByLabelText("Arguments") as HTMLTextAreaElement).value).toBe(raw);
    fireEvent.click(screen.getByRole("button", { name: "Form" }));
    fireEvent.click(screen.getByRole("button", { name: /Run tool/ }));
    await waitFor(() => expect(calls.at(-1)?.arguments).toEqual({ repository: "beta" }));
  });

  it.each([{ repository: "beta", unexpected: { nested: true } }, { repository: 42 }])(
    "preserves unsupported arguments %j for deliberate raw requests",
    async (args) => {
      const calls = await startApp();
      openDeveloper();
      fireEvent.click(screen.getByRole("button", { name: "Raw JSON" }));
      fireEvent.change(screen.getByLabelText("Arguments"), {
        target: { value: JSON.stringify(args) },
      });
      expect((screen.getByRole("button", { name: "Form" }) as HTMLButtonElement).disabled).toBe(
        true,
      );
      expect(screen.getByText(/Form unavailable: this draft contains fields/)).toBeTruthy();
      fireEvent.click(screen.getByRole("button", { name: "Raw JSON" }));
      fireEvent.click(screen.getByRole("button", { name: /Run tool/ }));
      await waitFor(() => expect(calls.at(-1)?.arguments).toEqual(args));
    },
  );

  it("retains invalid JSON per tool and never sends stale form arguments", async () => {
    const calls = await startApp();
    openDeveloper();
    fireEvent.change(screen.getByLabelText(/Repository name/), { target: { value: "alpha" } });
    fireEvent.click(screen.getByRole("button", { name: "Raw JSON" }));
    fireEvent.change(screen.getByLabelText("Arguments"), { target: { value: '{"repository":' } });
    expect((screen.getByRole("button", { name: "Form" }) as HTMLButtonElement).disabled).toBe(true);
    expect(
      screen.getByText(/Form unavailable: arguments must be a valid JSON object/),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /Run tool/ }));
    expect(screen.getByRole("alert").textContent).toContain("valid JSON object");
    expect(calls).toHaveLength(0);

    fireEvent.change(screen.getByLabelText("Tool"), { target: { value: "compare_standards" } });
    fireEvent.change(screen.getByLabelText(/Repository names/), {
      target: { value: "alpha, beta" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Raw JSON" }));
    fireEvent.change(screen.getByLabelText("Arguments"), {
      target: { value: '{"repositories":["beta","alpha"]}' },
    });
    fireEvent.click(screen.getByRole("button", { name: "Form" }));
    expect((screen.getByLabelText(/Repository names/) as HTMLInputElement).value).toBe(
      "beta, alpha",
    );
    fireEvent.change(screen.getByLabelText("Tool"), { target: { value: "audit_repository" } });
    expect((screen.getByLabelText("Arguments") as HTMLTextAreaElement).value).toBe(
      '{"repository":',
    );
    fireEvent.change(screen.getByLabelText("Tool"), { target: { value: "compare_standards" } });
    expect((screen.getByLabelText(/Repository names/) as HTMLInputElement).value).toBe(
      "beta, alpha",
    );
    fireEvent.click(screen.getByRole("button", { name: /Run tool/ }));
    await waitFor(() =>
      expect(calls.at(-1)?.arguments).toEqual({ repositories: ["beta", "alpha"] }),
    );
  });
});

it("runs discovery explicitly, selecting does not call tools, and validates developer JSON", async () => {
  const project = {
    name: "element",
    full_name: "example/element",
    description: "Demo",
    language: "TypeScript",
    archived: false,
    default_branch: "main",
  };
  const fetchMock = vi.fn(async (url: string, options?: RequestInit) => ({
    ok: true,
    json: async () =>
      url === "/api/call"
        ? { durationMs: 3, result: { structuredContent: { projects: [project] }, content: [] } }
        : { connected: true, busy: false, mode: "fixture", csrf: "test-token", tools: [] },
    requestBody: options?.body,
  }));
  vi.stubGlobal("fetch", fetchMock);
  render(<App />);
  await screen.findByText("Connected");
  expect(fetchMock.mock.calls.filter(([url]) => url === "/api/call")).toHaveLength(0);
  fireEvent.click(screen.getByRole("button", { name: "Discover" }));
  await screen.findByRole("button", { name: "Open element" });
  fireEvent.click(screen.getByRole("button", { name: "Open element" }));
  expect(screen.getByText("example/element")).toBeTruthy();
  expect(fetchMock.mock.calls.filter(([url]) => url === "/api/call")).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "MCP testing panel" }));
  fireEvent.click(screen.getByRole("button", { name: "Raw JSON" }));
  fireEvent.change(screen.getByLabelText("Arguments"), { target: { value: "[1]" } });
  fireEvent.click(screen.getByRole("button", { name: /Run tool/ }));
  await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("valid JSON object"));
  expect(fetchMock.mock.calls.filter(([url]) => url === "/api/call")).toHaveLength(1);
});
