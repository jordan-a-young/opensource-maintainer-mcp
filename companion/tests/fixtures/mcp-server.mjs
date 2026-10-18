// Deliberately offline MCP fixture. No GitHub client, credentials, or network calls.
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { CallToolRequestSchema, ListToolsRequestSchema } from "@modelcontextprotocol/sdk/types.js";

const scenario = process.argv[2] ?? "normal";
const names = [
  "list_projects",
  "get_project_overview",
  "audit_repository",
  "compare_standards",
  "analyze_ci",
  "review_release_config",
];
const projects = [
  ["element", "TypeScript", "Accessible building blocks for product teams."],
  ["availity-react", "TypeScript", "Shared React components and application utilities."],
  ["portal", "JavaScript", "A public workspace with intentionally incomplete metadata."],
  ["toolchain", "Rust", "Small tools for repeatable maintainer workflows."],
].map(([name, language, description]) => ({
  name,
  full_name: `example/${name}`,
  description,
  language,
  archived: false,
  default_branch: "main",
  url: `https://github.com/example/${name}`,
}));
const scope =
  "Offline fixture: bounded root-level observations only. Unknown states are not findings; these sample repositories are not live GitHub results.";
const check = (id, status, summary, evidence = [], recommendation = null) => ({
  id,
  status,
  summary,
  evidence,
  recommendation,
});
const checks = [
  check("documentation.readme", "passed", "A regular root README is observed.", ["README.md"]),
  check(
    "npm.test_script",
    "finding",
    "No conventional test script is declared in the parsed root manifest.",
    ["package.json#/scripts"],
    "Review whether root testing is provided through another mechanism.",
  ),
  check(
    "coverage.limitations",
    "not_assessed",
    "Test execution and test coverage were not inspected.",
  ),
  check("manifest.Cargo.toml", "not_applicable", "No root Cargo manifest was observed."),
];
const input = (name) => ({
  type: "object",
  properties:
    name === "list_projects"
      ? {}
      : name === "compare_standards"
        ? { repositories: { type: "array", items: { type: "string" }, minItems: 2, maxItems: 5 } }
        : { repository: { type: "string" } },
  required:
    name === "list_projects" ? [] : [name === "compare_standards" ? "repositories" : "repository"],
  additionalProperties: false,
});
const server = new Server(
  { name: "maintainer-offline-fixture", version: "1.0.0" },
  { capabilities: { tools: {} } },
);
server.setRequestHandler(ListToolsRequestSchema, () => ({
  tools: names.map((name) => ({
    name,
    description: `Offline fixture for ${name}. No GitHub requests are made.`,
    inputSchema: input(name),
    outputSchema: { type: "object", additionalProperties: true },
  })),
}));
server.setRequestHandler(CallToolRequestSchema, async (request) => {
  if (scenario === "large-result")
    return { structuredContent: { oversized: "x".repeat(2 * 1024 * 1024) }, content: [] };
  if (scenario === "hang") return new Promise(() => {});
  if (scenario === "exit") {
    process.exit(1);
  }
  if (scenario === "overflow") {
    process.stdout.write("x".repeat(5 * 1024 * 1024));
    return new Promise(() => {});
  }
  if (scenario === "malformed") {
    process.stdout.write("not JSON\n");
    return new Promise(() => {});
  }
  const { name, arguments: args = {} } = request.params;
  let data;
  if (name === "list_projects")
    data = { projects, ...(scenario === "lifecycle" ? { fixture_pid: process.pid } : {}) };
  else if (name === "compare_standards") {
    const selected = args.repositories;
    if (
      !Array.isArray(selected) ||
      selected.length < 2 ||
      selected.length > 5 ||
      new Set(selected).size !== selected.length ||
      selected.some((n) => !projects.some((p) => p.name === n))
    )
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: "Select 2–5 distinct fixture repositories returned by list_projects.",
          },
        ],
      };
    data = {
      scope,
      projects: selected.map((name) => ({
        full_name: `example/${name}`,
        tree_sha: "offline-snapshot",
      })),
      rows: [
        {
          id: "npm.optional_dependency.example",
          status: "different",
          observations: selected.map((name, i) => ({
            repository: `example/${name}`,
            status: i ? "absent" : "observed",
            values: i ? [] : ["^1.0.0"],
            evidence: ["package.json#/optionalDependencies"],
          })),
        },
        {
          id: "npm.package_manager",
          status: "different",
          observations: selected.map((name, i) => ({
            repository: `example/${name}`,
            status: "observed",
            values: [i ? "npm@11.16.0" : "pnpm@10.0.0"],
            evidence: ["package.json#/packageManager"],
          })),
        },
        {
          id: "npm.dependency.react",
          status: "incomplete",
          observations: selected.map((name, i) => ({
            repository: `example/${name}`,
            status: i ? "not_assessed" : "observed",
            values: i ? [] : ["^19"],
            evidence: i ? [] : ["package.json#/dependencies/react"],
          })),
        },
        {
          id: "cargo.workspace",
          status: "not_comparable",
          observations: selected.map((name) => ({
            repository: `example/${name}`,
            status: "not_applicable",
            values: [],
            evidence: [],
          })),
        },
      ],
    };
  } else {
    const project = projects.find((p) => p.name === args.repository);
    if (!project)
      return {
        isError: true,
        content: [
          {
            type: "text",
            text: "Repository is outside the configured fixture scope. Discover projects and use a returned simple name.",
          },
        ],
      };
    const common = { project, tree_sha: "offline-snapshot", scope };
    if (name === "get_project_overview")
      data = {
        project,
        tree_sha: "offline-snapshot",
        topics: ["maintainer-demo", "opensource"],
        manifests: [
          {
            path: "package.json",
            status: "parsed",
            name: `@example/${project.name}`,
            version: "1.2.0",
            package_manager: "npm@11.16.0",
            workspace: false,
            scripts: { build: "example-build-command (data only)" },
          },
        ],
        configuration_files: ["tsconfig.json", "package-lock.json", ".gitignore"],
      };
    else if (name === "audit_repository") data = { ...common, checks };
    else if (name === "analyze_ci")
      data = {
        ...common,
        checks: [
          check("workflows.discovery", "passed", "A workflow candidate was observed.", [
            ".github/workflows/ci.yml",
          ]),
        ],
        workflows: [
          {
            path: ".github/workflows/ci.yml",
            status: "analyzed",
            triggers: ["push", "pull_request"],
            jobs: [{ id: "checks", reusable_workflow: false, step_count: 4 }],
            checks: [
              check(
                "job.checks.permissions",
                "not_assessed",
                "Inherited repository permission defaults are unknown.",
              ),
              check(
                "job.checks.timeout",
                "finding",
                "No explicit numeric timeout is declared.",
                [".github/workflows/ci.yml#/jobs/checks"],
                "Review whether an explicit timeout is appropriate.",
              ),
            ],
          },
        ],
      };
    else if (name === "review_release_config")
      data = {
        ...common,
        package: {
          status: "observed",
          name: `@example/${project.name}`,
          version: "1.2.0",
          private: false,
          workspace: false,
          publish_access: "public",
          registry: "npm_https",
          provenance: null,
          script_names: ["release"],
          tooling: ["@changesets/cli"],
        },
        configuration_files: [".changeset"],
        workflows: [{ path: ".github/workflows/release.yml", status: "analyzed" }],
        release_jobs: [
          {
            workflow: ".github/workflows/release.yml",
            job: "publish",
            indicators: ["npm_publish"],
            oidc_permission: "observed",
            token_environment_present: false,
            checks: [
              check(
                "release.trust",
                "not_assessed",
                "npm account-side trust and actual publication remain unverified.",
              ),
            ],
          },
        ],
        checks: [
          check("release.package", "passed", "Root name and version declarations are observed.", [
            "package.json#/name",
            "package.json#/version",
          ]),
        ],
      };
    else return { isError: true, content: [{ type: "text", text: "Unknown fixture tool." }] };
  }
  return { structuredContent: data, content: [{ type: "text", text: JSON.stringify(data) }] };
});
if (scenario === "stderr") process.stderr.write("SECRET_FIXTURE_DIAGNOSTIC\n");
if (scenario !== "no-init") await server.connect(new StdioServerTransport());
else process.stdin.resume();
