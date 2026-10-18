import { array, record, text, type ToolResult } from "./types";

export function Status({ value }: { value: unknown }) {
  const label = text(value);
  const tone = ["passed", "observed", "parsed", "analyzed", "same"].includes(label)
    ? "positive"
    : ["finding", "different", "invalid"].includes(label)
      ? "attention"
      : "neutral";
  return <span className={`badge ${tone}`}>{label.replaceAll("_", " ")}</span>;
}

function Values({ values, empty = "None observed" }: { values: unknown; empty?: string }) {
  const items = array(values);
  return items.length ? (
    <ul className="value-list">
      {items.map((value, i) => (
        <li key={i}>{text(value)}</li>
      ))}
    </ul>
  ) : (
    <span className="muted">{empty}</span>
  );
}

function Facts({ value }: { value: unknown }) {
  return (
    <dl className="facts">
      {Object.entries(record(value)).map(([key, item]) => (
        <div key={key}>
          <dt>{key.replaceAll("_", " ")}</dt>
          <dd>{Array.isArray(item) ? <Values values={item} /> : text(item)}</dd>
        </div>
      ))}
    </dl>
  );
}

function Checks({ value }: { value: unknown }) {
  const checks = array(value);
  return checks.length ? (
    <div className="checks">
      {checks.map((item, i) => {
        const check = record(item);
        return (
          <article className="check" key={i}>
            <div className="check-heading">
              <code>{text(check.id)}</code>
              <Status value={check.status} />
            </div>
            <p>{text(check.summary)}</p>
            {array(check.evidence).length > 0 && (
              <details>
                <summary>Observed evidence ({array(check.evidence).length})</summary>
                <Values values={check.evidence} />
              </details>
            )}
            {typeof check.recommendation === "string" && (
              <p className="recommendation">{check.recommendation}</p>
            )}
          </article>
        );
      })}
    </div>
  ) : (
    <p className="muted">No checks returned. This is not a repository-wide verdict.</p>
  );
}

export function Report({ result }: { result: ToolResult }) {
  const data = record(result.structuredContent);
  // These are separate observations; repeated rule IDs in different workflows/jobs
  // must each count. CI job checks live on their workflow, not on the job summary.
  const checks = [
    ...array(data.checks),
    ...array(data.workflows).flatMap((workflow) => array(record(workflow).checks)),
    ...array(data.release_jobs).flatMap((job) => array(record(job).checks)),
  ].map(record);
  return (
    <div className="report">
      {result.isError ? (
        <div className="notice error">
          The MCP server returned a tool error. No report was inferred.
        </div>
      ) : null}
      {typeof data.scope === "string" && <p className="scope">{data.scope}</p>}
      {data.project ? <Facts value={data.project} /> : null}
      {data.tree_sha ? (
        <p className="snapshot">
          Snapshot <code>{text(data.tree_sha)}</code>
        </p>
      ) : null}
      {checks.length > 0 && (
        <section aria-label="Check status counts">
          <h3>All reported checks</h3>
          <p className="hint">Totals include root/discovery, workflow, and release-job checks.</p>
          <div className="metrics">
            {["passed", "finding", "not_assessed", "not_applicable"].map((status) => (
              <div key={status}>
                <strong>{checks.filter((c) => c.status === status).length}</strong>
                <span>{status.replaceAll("_", " ")}</span>
              </div>
            ))}
          </div>
        </section>
      )}
      {data.topics ? (
        <section>
          <h3>Topics</h3>
          <Values values={data.topics} />
        </section>
      ) : null}
      {data.manifests ? (
        <section>
          <h3>Root manifests</h3>
          {array(data.manifests).length === 0 ? (
            <p className="muted">No supported root manifests observed.</p>
          ) : (
            array(data.manifests).map((item, i) => {
              const m = record(item);
              return (
                <article className="subcard" key={i}>
                  <div className="check-heading">
                    <h4>{text(m.path)}</h4>
                    <Status value={m.status} />
                  </div>
                  <Facts
                    value={{
                      name: m.name,
                      version: m.version,
                      package_manager: m.package_manager,
                      workspace: m.workspace,
                    }}
                  />
                  <details>
                    <summary>Script declarations (never executed)</summary>
                    <Facts value={m.scripts} />
                  </details>
                </article>
              );
            })
          )}
        </section>
      ) : null}
      {data.configuration_files ? (
        <section>
          <h3>Configuration filenames</h3>
          <Values values={data.configuration_files} />
        </section>
      ) : null}
      {data.package ? (
        <section>
          <h3>Root publication metadata</h3>
          <Facts value={data.package} />
        </section>
      ) : null}
      {data.checks ? (
        <section>
          <h3>Observed checks</h3>
          <Checks value={data.checks} />
        </section>
      ) : null}
      {data.rows ? (
        <section>
          <h3>Standards comparison</h3>
          <p className="muted">
            Differences are descriptive. Declarations are compared literally; compatibility is not
            inferred.
          </p>
          <details>
            <summary>Repository snapshots</summary>
            {array(data.projects).map((p, i) => (
              <Facts key={i} value={p} />
            ))}
          </details>
          <div className="table-scroll" tabIndex={0} aria-label="Standards comparison table">
            <table>
              <thead>
                <tr>
                  <th>Convention / declaration</th>
                  <th>Comparison</th>
                  {array(data.projects).map((p, i) => (
                    <th key={i}>{text(record(p).full_name)}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {array(data.rows).map((item, i) => {
                  const row = record(item);
                  return (
                    <tr key={i}>
                      <th scope="row">
                        <code>{text(row.id)}</code>
                      </th>
                      <td>
                        <Status value={row.status} />
                      </td>
                      {array(data.projects).map((p, j) => {
                        const obs = array(row.observations)
                          .map(record)
                          .find((o) => o.repository === record(p).full_name);
                        return (
                          <td key={j}>
                            {obs ? (
                              <>
                                <Status value={obs.status} />
                                <Values values={obs.values} empty="No values reported" />
                                <details>
                                  <summary>Evidence</summary>
                                  <Values values={obs.evidence} />
                                </details>
                              </>
                            ) : (
                              <Status value="not_assessed" />
                            )}
                          </td>
                        );
                      })}
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>
      ) : null}
      {data.workflows ? (
        <section>
          <h3>Workflow inspection</h3>
          {array(data.workflows).length === 0 ? (
            <p className="muted">
              No workflow summaries returned; consult discovery checks and scope.
            </p>
          ) : (
            array(data.workflows).map((item, i) => {
              const workflow = record(item);
              return (
                <article className="subcard" key={i}>
                  <div className="check-heading">
                    <h4>{text(workflow.path)}</h4>
                    <Status value={workflow.status} />
                  </div>
                  {workflow.triggers ? (
                    <>
                      <h5>Triggers</h5>
                      <Values values={workflow.triggers} />
                    </>
                  ) : null}
                  {workflow.jobs ? (
                    <>
                      <h5>Jobs</h5>
                      {array(workflow.jobs).map((job, j) => (
                        <Facts key={j} value={job} />
                      ))}
                    </>
                  ) : null}
                  {workflow.checks ? <Checks value={workflow.checks} /> : null}
                </article>
              );
            })
          )}
        </section>
      ) : null}
      {data.release_jobs ? (
        <section>
          <h3>Release job indicators</h3>
          <p className="muted">
            OIDC permission does not establish account-side trusted publishing. Workspace members
            and actual publication remain unverified.
          </p>
          {array(data.release_jobs).length === 0 ? (
            <p>No release job indicators observed.</p>
          ) : (
            array(data.release_jobs).map((item, i) => {
              const job = record(item);
              const { checks: jobChecks, ...facts } = job;
              return (
                <article className="subcard" key={i}>
                  <Facts value={facts} />
                  <Checks value={jobChecks} />
                </article>
              );
            })
          )}
        </section>
      ) : null}
      {!result.structuredContent && (
        <section>
          <h3>MCP content</h3>
          {array(result.content).map((c, i) => (
            <pre key={i}>{text(record(c).text ?? c)}</pre>
          ))}
        </section>
      )}
      <details className="raw">
        <summary>Raw MCP result</summary>
        <pre>{JSON.stringify(result, null, 2)}</pre>
      </details>
    </div>
  );
}
