export const TOOLS = [
  {
    name: "list_projects",
    label: "Discover projects",
    short: "Discovery",
    description: "List public repositories in the configured owner and topic.",
  },
  {
    name: "get_project_overview",
    label: "Project overview",
    short: "Overview",
    description: "Root manifests, metadata, scripts, and configuration filenames.",
  },
  {
    name: "audit_repository",
    label: "Repository audit",
    short: "Audit",
    description: "Observed maintenance checks with evidence and recommendations.",
  },
  {
    name: "compare_standards",
    label: "Compare standards",
    short: "Comparison",
    description: "Compare literal declarations across two to five projects.",
  },
  {
    name: "analyze_ci",
    label: "Analyze CI",
    short: "CI",
    description: "Bounded, static inspection of GitHub Actions workflows.",
  },
  {
    name: "review_release_config",
    label: "Review release config",
    short: "Release",
    description: "Root npm metadata and workflow publication indicators.",
  },
] as const;
export type ToolName = (typeof TOOLS)[number]["name"];
export interface ToolSchema {
  name: string;
  description?: string;
  inputSchema: unknown;
  outputSchema?: unknown;
}
export interface Session {
  connected: boolean;
  busy: boolean;
  mode: "local" | "fixture";
  tools: ToolSchema[];
  csrf?: string;
}
export interface ToolResult {
  structuredContent?: unknown;
  content?: unknown;
  isError?: boolean;
}
export interface CallResponse {
  result: ToolResult;
  durationMs: number;
}
export interface Entry {
  id: number;
  name: ToolName;
  args: Record<string, unknown>;
  time: string;
  durationMs: number;
  response?: ToolResult;
  error?: string;
}
export interface Project {
  name: string;
  full_name: string;
  description: string | null;
  language: string | null;
  archived: boolean;
  default_branch: string;
}
export const record = (value: unknown): Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
export const array = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
export const text = (value: unknown): string =>
  value === null || value === undefined
    ? "Unknown / not provided"
    : typeof value === "object"
      ? JSON.stringify(value)
      : typeof value === "string"
        ? value
        : typeof value === "boolean" || typeof value === "number"
          ? String(value)
          : "Unsupported value";
export function projectList(result: ToolResult): Project[] {
  return array(record(result.structuredContent).projects).flatMap((value) => {
    const p = record(value);
    return typeof p.name === "string" && typeof p.full_name === "string"
      ? [
          {
            name: p.name,
            full_name: p.full_name,
            description: typeof p.description === "string" ? p.description : null,
            language: typeof p.language === "string" ? p.language : null,
            archived: p.archived === true,
            default_branch: typeof p.default_branch === "string" ? p.default_branch : "Unknown",
          },
        ]
      : [];
  });
}
export function retainHistory(entries: Entry[]): Entry[] {
  let bytes = 0;
  return entries.slice(0, 12).filter((entry) => {
    bytes += new TextEncoder().encode(JSON.stringify(entry)).byteLength;
    return bytes <= 4 * 1024 * 1024;
  });
}
