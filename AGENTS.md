# Contributor guidance

Implement one milestone per pull request. Milestones 1–3 expose `list_projects`,
`get_project_overview`, and `audit_repository`.
Do not implement roadmap tools without explicit approval. Never merge without approval.

Keep MCP handlers in `src/mcp.rs`, HTTP integration in `src/github.rs`, business logic
in `src/discovery.rs`, `src/overview.rs`, and `src/audit.rs`, and configuration/models/errors in their
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
