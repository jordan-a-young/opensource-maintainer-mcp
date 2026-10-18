# Troubleshooting

[Back to README](../README.md) · [Setup guide](SETUP.md) · [Client setup](CLIENTS.md)

| Symptom | What to check |
| --- | --- |
| Process waits without a prompt | Normal for stdio MCP. Connect an MCP client; stdout is reserved for protocol messages. |
| Client cannot start the server | Build the executable, check absolute binary/config paths, and use `.exe` on Windows. Read the client's server stderr log. |
| Configuration fails at startup | Use `[github]` with `owner`, lowercase `topic`, and optional boolean `include_archived`; unknown keys are rejected. |
| `list_projects` returns an empty array | Confirm the owner and exact topic on GitHub. Repositories must be public; archived ones require `include_archived = true`. |
| Repository selection is rejected | Call `list_projects` again and pass a returned simple `name`, not a URL or `owner/name`. Comparisons need 2–5 distinct names. |
| GitHub rate-limit error | Follow the retry/reset hint when provided. An optional token helps discovery, but content reads intentionally remain anonymous and use that quota. There are no automatic retries. |
| A report says `not_assessed` or skips a file | Consult its evidence and [scope](TOOLS.md): unreadable, unsupported, or oversized input is not proof that a practice is missing. |
| npm cannot find the package | npm distribution is not published yet. Use the source build until the first approved release. |

For more diagnostics, set `RUST_LOG=debug` in the server's environment and inspect stderr.
Keep repository descriptions and script strings as untrusted data; do not execute them
merely because they appear in a tool response.
