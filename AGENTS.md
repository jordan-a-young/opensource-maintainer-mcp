# Contributor guidance

## Scope and development entry points

This is a read-only stdio MCP server for bounded static inspection of public GitHub
repositories. Start with `README.md`; see `docs/SETUP.md` for setup and supported
platforms, `docs/TOOLS.md` for tool contracts, and `docs/DEVELOPMENT.md` for test/check
commands. Use stable Rust and the committed `Cargo.lock`; source builds
use `cargo build --locked`. The `config.toml` workflow and sanitized example are
documented in `docs/SETUP.md` and `examples/config.toml`. Do not install toolchains or
change dependencies merely to run a check without approval.

Read `docs/RELEASING.md` and `docs/NPM.md` before release or distribution work.
The architecture, verification, security, and approval constraints below apply
alongside that documentation. Preserve existing tool contracts and stable rule IDs;
keep unknown/inapplicable states distinct from observed findings.

## Architecture and verification

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

Release infrastructure is separate from the read-only MCP server. Keep publication manual:
version tags may prepare drafts, but workflows must not publish a release or upload to
crates.io without explicit approval. Share artifact builds between CI and release workflows,
pin external actions, and validate the archive layout and checksums when changing packaging.

For npm distribution, keep the source manifest private and generate publishable copies
under ignored npm-dist/. No postinstall downloads or install-time compilation. Reserve
launcher stdout for MCP and pin native dependencies to the exact version. Never upload,
stage (which can publish placeholders), bootstrap packages, or enable the staging gate
without explicit approval. Trusted publishing account setup is separate from dry-run tests.
