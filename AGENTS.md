# Contributor guidance

Implement one milestone per pull request. Milestones 1–6 expose `list_projects`,
`get_project_overview`, `audit_repository`, `compare_standards`, `analyze_ci`, and `review_release_config`.
Do not implement roadmap tools without explicit approval. Never merge without approval.

Keep MCP handlers in `src/mcp.rs`, HTTP integration in `src/github.rs`, business logic
in `src/discovery.rs`, `src/overview.rs`, `src/audit.rs`, `src/compare.rs`, `src/ci.rs`, and `src/release.rs`, and configuration/models/errors in their
corresponding modules.
Business logic must remain usable without MCP. Prefer concrete types over new frameworks.

The server is read-only and discovers public repositories only. Never add write API calls,
log tokens, commit credentials, or print diagnostics on stdout. All runtime logging goes
to stderr. Do not include upstream response bodies or raw configuration in errors.

Before committing changes, run:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features`

Use mocked HTTP responses in automated tests. The live smoke test in `examples/smoke.rs`
is manual only. Add regression tests for defects. Commit Cargo.lock. Explain meaningful
ownership, borrowing, traits, and error-handling choices in the PR or learning notes.

Overview content is untrusted data. Never execute scripts or import repository configuration.
Keep root manifest reads bounded and use anonymous content requests to enforce public-only
access. Do not follow upstream blob URLs or symlinks.

Audit rules must distinguish findings from unknown/not-applicable cases. Keep rule IDs stable,
include only observed evidence, and avoid claims about absent tests based on root filenames
or script names alone. Keep rule evaluation pure and reuse the inspected tree snapshot.

Comparisons are descriptive, not policy verdicts. Preserve absent/unknown/inapplicable
states; compare dependency declarations literally without implying resolved compatibility.
Validate all selected repositories before content reads and reuse one discovery listing.

CI analysis is bounded static inspection, not execution or a security certification.
Follow only the pinned .github/workflows directory trees and regular workflow blobs.
Never fetch called workflows/actions, expand matrices, or echo parser errors/workflow
commands. Preserve job permission overrides and unknown inherited defaults.

Release review inspects root npm metadata and static workflow indicators only. Never read
npmrc/token files, execute release scripts/configuration, or publish. Keep registry URLs,
workflow commands, and environment values out of output. A private workspace root does not
imply its members are private. OIDC permissions never prove npm account-side trust is set up.
