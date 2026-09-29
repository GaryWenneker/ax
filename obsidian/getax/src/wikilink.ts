export type AxKind = "node" | "rule" | "skill" | "memory";

export interface AxLink {
  kind: AxKind;
  id: string;
}

const KINDS = new Set<AxKind>(["node", "rule", "skill", "memory"]);

export function parseAxLink(linktext: string): AxLink | null {
  const path = linktext.split("|")[0]?.split("#")[0]?.trim() ?? "";
  const parts = path.split("/").filter((part) => part.length > 0);
  if (parts[0] !== "ax" || parts.length < 2) return null;
  const head = parts[1];
  if (head === "rule" || head === "skill" || head === "memory") {
    const id = decodeURIComponent(parts.slice(2).join("/"));
    if (!id) return null;
    return { kind: head, id };
  }
  if (KINDS.has(head as AxKind) && parts.length === 2) return null;
  return { kind: "node", id: decodeURIComponent(parts.slice(1).join("/")) };
}

export function formatAxLink(kind: AxKind, id: string, alias: string): string {
  const safeAlias = alias.replace(/[\[\]|]/g, "").trim() || id;
  const path = kind === "node" ? `ax/${encodePath(id)}` : `ax/${kind}/${encodePath(id)}`;
  return `[[${path}|${safeAlias}]]`;
}

function encodePath(id: string): string {
  return id
    .split("/")
    .map((part) => encodeURIComponent(part))
    .join("/");
}
