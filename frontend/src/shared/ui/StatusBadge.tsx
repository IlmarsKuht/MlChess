import type { ReactNode } from "react";

export function StatusBadge({
  tone,
  children
}: {
  tone: "quiet" | "running" | "good" | "warning";
  children: ReactNode;
}) {
  return <span className={`status-badge status-${tone}`}>{children}</span>;
}
