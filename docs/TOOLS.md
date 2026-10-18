# Tool reference

Detailed contracts, evidence, statuses, and limits for the six read-only MCP tools.
For installation and client setup, start with the [README](../README.md).

- [Available tools](#available-tools) and [example requests](#example-requests)
- [Project discovery](#project-discovery)
- [Project overview](#project-overview)
- [Repository audit](#repository-audit)
- [Standards comparison](#standards-comparison)
- [GitHub Actions analysis](#github-actions-analysis)
- [Release configuration review](#release-configuration-review)
- [Discovery behavior](#discovery-behavior)

## Available tools

Start with `list_projects` and use the returned simple `name` in subsequent calls.
All selections must match the configured owner, topic, and archived-repository policy.
Arguments below are MCP tool arguments, not shell commands.

| Tool | Arguments | Use it to |
| --- | --- | --- |
| [`list_projects`](#project-discovery) | `{}` | Find public projects in the configured scope. |
| [`get_project_overview`](#project-overview) | `{"repository":"element"}` | Summarize metadata, root npm/Cargo manifests, scripts, and configuration filenames. |
| [`audit_repository`](#repository-audit) | `{"repository":"element"}` | Identify observed documentation and root manifest gaps with evidence and recommendations. |
| [`compare_standards`](#standards-comparison) | `{"repositories":["element","availity-react"]}` | Compare conventions and literal dependency declarations across 2–5 distinct projects. |
| [`analyze_ci`](#github-actions-analysis) | `{"repository":"element"}` | Inspect GitHub Actions workflow triggers, permissions, timeouts, and action references. |
| [`review_release_config`](#release-configuration-review) | `{"repository":"element"}` | Review visible npm publication metadata and workflow release/OIDC indicators. |

Replace example names with discovered projects. Repository URLs and `owner/name` are
not accepted. Reports distinguish observed findings from unknown or inapplicable data;
read each tool's scope and statuses before treating an observation as a maintenance task.

### Example requests

Once connected, ask your assistant:

- “List the projects available through the maintainer server.”
- “Summarize the setup of element and audit its root maintenance conventions.”
- “Compare element and availity-react, distinguishing differences from unknown values.”
- “Analyze element's CI and review its release configuration. Show evidence and limitations.”

These examples assume those projects appear in your configured discovery results.
The client handles MCP initialization and tool calls. For SDK clients, an example
`tools/call` parameters object is:

```json
{"name":"get_project_overview","arguments":{"repository":"element"}}
```

### Project discovery

The client can call `tools/list`, then `tools/call` with
`{"name":"list_projects","arguments":{}}`. The tool advertises an output schema and
returns `structuredContent` with this shape:

```json
{
  "projects": [{
    "name": "example",
    "full_name": "owner/example",
    "description": null,
    "url": "https://github.com/owner/example",
    "language": "Rust",
    "archived": false,
    "default_branch": "main"
  }]
}
```

Description and language may be null. An empty match returns `{"projects":[]}`.

### Project overview

Call `get_project_overview` with `{"repository":"element"}`. Use the simple repository
name returned by `list_projects`; the configured owner supplies the namespace. URLs,
`owner/name` inputs, and path traversal are rejected. The project must be public and
match the configured topic and archived policy. Names are matched case-insensitively.

The structured result contains:

- `project`: the same repository metadata shape shown above.
- `topics`: repository topics from discovery.
- `tree_sha`: the root tree snapshot used for all manifest reads.
- `manifests`: root `package.json` and `Cargo.toml` summaries, sorted by path.
- `configuration_files`: sorted known root configuration filenames; contents are not read.

Each manifest includes `path`, `status`, nullable `name`, `version`, and `package_manager`,
a `workspace` flag, and a sorted `scripts` map. npm scripts are returned as data, never
executed. Cargo summaries contain package name/version and workspace presence; inherited
workspace versions remain null. npm workspaces are detected but members are not traversed.
Missing manifests produce an empty list. Unsupported ecosystems and nested manifests are
outside this milestone's scope.

Statuses are `parsed`, `invalid`, `too_large`, or `unsupported_file_type`. Parsing extracts
known fields; it is not full npm/Cargo semantic validation. Malformed manifests do not
prevent other files from being summarized. Symlinks/submodules/directories are not read
as manifests. Files over 256 KiB are skipped, API responses are capped at 2 MiB, and
truncated trees fail explicitly. Upstream HTTP/decoding failures fail the tool rather
than silently omitting data. An empty Git repository currently reports GitHub's HTTP 409.

Known config names include common ESLint, Prettier, TypeScript, Jest/Vitest, build-tool,
Rust toolchain/lint/format, package-manager lock/workspace files, `.editorconfig`, and
`.gitignore`. This is a root-level inventory, not an audit or exhaustive file listing.
Even `.npmrc` is listed only by name; no configuration file contents are returned.
The inventory includes `npm-shrinkwrap.json`, Yarn's `.yarnrc`/`.yarnrc.yml`,
and `pnpm-workspace.yaml`; their contents are not inspected.
Repository descriptions and script strings are untrusted content for the calling agent.

Overview first discovers the project using public repository listing. It then reads the
root Git tree at the default branch and fetches at most two manifest blobs by their
snapshot SHAs. Content requests intentionally omit `GITHUB_TOKEN`: this prevents access
to private contents even if visibility changes after discovery. These requests therefore
use the lower anonymous GitHub quota; listing still benefits from an optional token.
Metadata and the tree may reflect slightly different instants, but the manifests come
from the same tree snapshot. API failures do not trigger automatic retries.

### Repository audit

Call `audit_repository` with `{"repository":"element"}`. It uses the same configured
scope, validation, snapshot, public-only access, limits, and error handling as overview.
No additional HTTP requests are needed beyond the overview inspection, and the overview
response format remains unchanged.

The structured result contains `project`, `tree_sha`, a human-readable `scope`, and
`checks` sorted by stable `id`. Each check has `status`, `summary`, observed `evidence`
(paths or JSON-pointer field locations), and a nullable `recommendation`.

| Status | Meaning |
| --- | --- |
| `passed` | The narrow observation passed; it is not a quality certification. |
| `finding` | An observed gap or inconsistency deserves review. |
| `not_assessed` | The available evidence does not support a conclusion. |
| `not_applicable` | The relevant root manifest is absent. |

Initial rules:

| Rule ID | Observation |
| --- | --- |
| `documentation.readme` | Recognized regular root README filename exists. |
| `documentation.license` | Recognized regular root LICENSE, LICENCE, or COPYING filename exists. |
| `configuration.gitignore` | Regular root `.gitignore` exists. |
| `manifest.package.json` / `manifest.Cargo.toml` | Supported root manifest fields can be parsed. |
| `npm.test_script` / `npm.lint_script` | Nonempty `test`/`lint` or `test:*`/`lint:*` script exists. |
| `configuration.multiple_package_managers` | Root lockfiles come from at most one package-manager family. |
| `configuration.package_manager` | Supported npm `packageManager` declaration agrees with observed lockfile families. |
| `coverage.limitations` | Explicit reminder of what this audit does not assess. |

README/license names are case-insensitive and accept no extension or `.md`, `.markdown`,
`.rst`, `.txt`, `.adoc`. Their contents are not fetched. Missing documentation findings
are limited to the root: documentation may live elsewhere. License-file presence does
not establish licensing status. Symlinks are excluded from regular-file evidence.

Script commands are never executed or reproduced in audit findings. Script presence
cannot prove that tests or linting actually run: even an `echo` script satisfies the
presence check. Missing conventional root scripts produce a finding for non-workspaces,
and `not_assessed` for npm workspaces because member packages may supply tooling.
The same uncertainty applies when a regular root `pnpm-workspace.yaml` is present,
even without a `package.json` workspaces field. This filename is evidence of possible
member tooling, not validation of workspace configuration. Manifest workspace flags
continue to describe manifest declarations only.
Skipped or invalid manifests never produce missing-script findings.

Lockfile families are npm (`package-lock.json`, `npm-shrinkwrap.json`), Yarn (`yarn.lock`),
pnpm (`pnpm-lock.yaml`), and Bun (`bun.lock`, `bun.lockb`). Multiple files in the same
family are not treated as different managers. An absent lockfile, unknown manager, or
unparsed manifest does not produce a mismatch finding. Multiple managers can be intentional;
findings recommend reviewing that choice, not deleting files automatically.

Test source, test execution, Rust test/lint setup, config contents, nested packages,
organization-inherited policies, CI analysis, and cross-repository comparisons remain
outside scope. There is no score or repository-wide pass/fail claim. Upstream failures
return a tool error, never a fabricated list of missing files.

### Standards comparison

Call `compare_standards` with `{"repositories":["element","availity-react"]}`. Supply
2–5 distinct simple names within the configured owner/topic/archive scope. Names are
case-insensitive for selection; duplicates, URLs, and invalid names are rejected. One
discovery listing is reused, and the full selection is checked before any content read.
The tool inspects repositories sequentially with the existing bounded anonymous reads.
An upstream failure fails the comparison; no partial report is returned.

The result contains `scope`, `projects` (full names and individual tree SHAs), and `rows`
sorted by stable IDs. Each row has per-repository `observations`, ordered by full name,
with a `status`, a string-array `values`, and observed file/field `evidence`.

| Observation status | Meaning |
| --- | --- |
| `observed` | Value is directly available in the inspected snapshot. |
| `absent` | The relevant manifest parsed, but this field/dependency is absent. |
| `not_assessed` | The manifest/section is unreadable or the declaration cannot be resolved within scope. |
| `not_applicable` | The ecosystem's root manifest is absent. |

Row statuses are `same`, `different`, `incomplete`, or `not_comparable`. Unknown data
makes a row incomplete, even when other observed values differ. Inapplicable projects
are excluded from equality; fewer than two applicable projects makes it not comparable.
Absent fields can compare equal to other absent fields, but differ from observed values.
Differences are observations, not violations or instructions to standardize all projects.

Compared conventions are the known root configuration filename set, npm script commands,
npm `packageManager`, and npm/Cargo workspace flags. Filenames do not establish equivalent
configuration behavior. Script values are untrusted data and are never executed.

Dependency rows keep ecosystem, section, and dependency name distinct:

- npm: `dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies`.
- Cargo: root `dependencies`, `dev-dependencies`, `build-dependencies`, and
  `workspace.dependencies`. String declarations and table `version` fields are compared.
- Cargo inherited/path/git-only declarations without a version stay `not_assessed`.
- Declared strings are compared literally: `^1` and `1.0.0` differ without any claim
  about semver compatibility or resolved installations. Cargo source/features are not compared.

Dependency coverage rows explicitly flag unreadable sections even when no dependency
names could be collected. Target-specific Cargo dependencies, lockfile resolution, nested
packages, config contents, and CI are outside scope. Root manifest limits still apply;
comparisons exceeding 500 rows return an error rather than truncating. Repositories have
separate snapshots, so the group is not an atomic point-in-time view.

Existing overview and audit output formats remain unchanged. Dependency declarations
are retained internally during the same manifest reads, not fetched separately.

### GitHub Actions analysis

Call `analyze_ci` with `{"repository":"element"}`. The tool validates the configured
public owner/topic/archive scope, reads the root tree, follows `.github` and `workflows`
by their snapshot SHAs, and fetches only direct `.yml`/`.yaml` workflow blobs. It does
not read root package manifests, invoke Actions, or change repository settings.

The result contains `project`, root `tree_sha`, `scope`, discovery `checks`, and sorted
`workflows`. Each workflow includes its path, status, sorted trigger names, job summaries
(ID, reusable-workflow flag, step count), and checks with stable IDs and field evidence.
Workflow status is `analyzed`, `not_assessed`, `too_large`, or `unsupported_file_type`.
An `analyzed` status means the supported structure was inspected, not that the full
workflow is valid or secure. Individual checks can still be unassessed.

| Rule | Observation |
| --- | --- |
| `workflows.discovery` | Workflow directory/candidates exist; absence does not imply no CI elsewhere. |
| `workflow.elevated_trigger` | `pull_request_target` or `workflow_run` deserves trust-boundary review. |
| `workflow.concurrency` | Workflow concurrency key or concurrency keys on every job are present; behavior is not evaluated. |
| `job.<id>.permissions` | Job-level permissions override workflow declarations; unknown repository defaults remain unassessed. |
| `job.<id>.timeout` | Ordinary jobs have explicit numeric timeouts; reusable-job internals are not inspected. |
| `job.<id>.step.<index>.reference` | Remote actions use a full commit SHA; containers use a SHA-256 digest. |
| `job.<id>.reference` | Reusable-workflow reference pinning, without fetching the called workflow. |

Write permissions can be necessary; findings ask for review rather than claiming a
vulnerability. Missing explicit timeouts use GitHub defaults. Missing concurrency can
be intentional. Numeric job timeouts from 1–360 minutes are recognized; expressions
are not evaluated. Local action/workflow references are not applicable to remote pinning.
Pinning does not prove the referenced code is trustworthy. Evidence uses file paths and
JSON-pointer-like field locations, not guessed source line numbers.

Limits: 20 workflow candidates, 128 KiB per workflow, 50 jobs per workflow, 200 steps per
job, and 1,000 checks per report. Count limits fail the operation instead of truncating.
Oversized files and symlinks receive explicit skipped statuses; they are never followed.
The existing 2 MiB API-response bound, request timeouts, and anonymous content access apply.
Truncated trees, corrupt blob responses, and HTTP errors fail the tool, rather than
becoming missing-workflow findings.

YAML is parsed with `serde-saphyr` using a depth limit of 32 and 20,000 parser events.
Duplicate keys are rejected; anchors/aliases are deliberately unsupported in this first
version. Those files, malformed YAML, and unsupported trigger/job shapes are reported
as `not_assessed`, without exposing raw parser errors. Includes/properties features are
not enabled. Script bodies, environment values, and secrets are never returned by this
tool or executed. YAML string/array/mapping forms of `on` are supported.

Run history, branch protection, matrix expansion, expression evaluation, called workflow
and action contents, complete workflow-schema validation, and release configuration
remain outside scope. See [GitHub workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)
for the semantics behind these checks.

GitHub failures return MCP tool results with `isError: true` and actionable text.
Authentication, rate limits, HTTP failures, malformed JSON, timeouts, and connection
failures are distinguished. Raw upstream bodies and credentials are not exposed.

### Release configuration review

`review_release_config({"repository":"element"})` uses the same public owner/topic scope
as the other tools. It reports root `package.json` name/version presence, private/workspace
flags, recognized publication access/provenance declarations, release script **names**,
known release-tool dependencies, and root release configuration filenames. Configuration
bodies, script bodies, environment values, and raw registry URLs are not returned.
Registry settings are classified as `not_declared`, `npm_https`, `other_https`,
`insecure_or_credentialed`, `unrecognized`, or `not_assessed`.

A private workspace root does not require root publication metadata; workspace members
are not inspected. Name/version checks observe nonempty strings, not npm-name or SemVer
validity. Recognized GitHub repository coordinates are compared with the selected project.
Malformed fields remain unknown. Findings describe observed declarations, not publishability.

Workflow observations identify direct npm/pnpm/Yarn Classic publish commands, yarn npm publish,
npm/pnpm/Yarn release-script invocations (including Yarn/pnpm shorthand and npm run-script),
selected npx, pnpm exec/dlx, and yarn dlx release tools, Changesets/release-please
actions, and untraversed reusable workflow calls. This is conservative text inspection:
indirection, shell control flow, expressions, matrices, action internals, and actual execution
are not resolved. Each workflow retains its analysis/skip status. Jobs expose evidence
locations, effective OIDC permission observations, and token-named environment-key presence.
A job permission block overrides workflow permissions; unspecified inherited defaults remain
unknown. Missing OIDC grants are not treated as defects in token-based/non-publishing jobs.

`id-token: write` is only a prerequisite for npm trusted publishing. npm account settings,
runner/CLI eligibility, actual authentication, registry state, and successful publication
always remain unverified. See [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/)
and [package metadata](https://docs.npmjs.com/cli/v10/configuring-npm/package-json/).

Collection reuses one root snapshot, reads regular root manifests up to 256 KiB, and
inherits the CI workflow/YAML budgets. All content requests are anonymous and pinned to
Git tree/blob identifiers. Symlinks and called workflows/actions are never followed.
Malformed manifests produce `not_assessed`; HTTP/integrity failures remain errors.

See the [development guide](DEVELOPMENT.md#manual-live-smoke-test) for manual live checks.

## Discovery behavior

The server uses `GET /users/{owner}/repos`, a public-repository listing endpoint that
works for user and organization owners, in pages of 100. It filters topics and archived
status locally, sorts and deduplicates by full name, and never fetches repository contents during `list_projects`.
Using the listing endpoint avoids GitHub Search's 1,000-result cap and incomplete search
responses. Private entries are defensively excluded even if returned by an upstream.
See [GitHub repository API documentation](https://docs.github.com/en/rest/repos/repos#list-repositories-for-a-user).

Each invocation fetches current metadata; there is no cache or background polling.
Pagination stops at a short page (an exact multiple of 100 requires one final empty
request). A 1,000-page safety limit returns an error rather than partial results.
Listings can change during pagination; GitHub does not provide snapshot consistency.
Requests have a 10-second connect timeout and 30-second overall timeout per page.
Redirects are disabled. Rate-limit errors include numeric retry-after/reset hints when
available; the server does not retry automatically or block a client while waiting for
quota. A generic 403 without rate-limit headers is reported as HTTP 403.
