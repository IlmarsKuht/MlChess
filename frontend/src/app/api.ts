const apiBase = import.meta.env.VITE_API_BASE ?? "/api";

export function apiUrl(path: string) {
  return `${apiBase}${path}`;
}

export function wsUrl(path: string) {
  const base = apiBase.startsWith("http")
    ? new URL(apiBase)
    : new URL(apiBase, window.location.origin);
  base.protocol = base.protocol === "https:" ? "wss:" : "ws:";
  base.pathname = `${base.pathname.replace(/\/$/, "")}${path}`;
  base.search = "";
  base.hash = "";
  return base.toString();
}

export async function fetchJson<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(apiUrl(path), {
    headers: {
      "Content-Type": "application/json"
    },
    ...init
  });
  if (!response.ok) {
    const error = await response.json().catch(() => ({ error: "Request failed" }));
    throw new Error(error.error ?? "Request failed");
  }
  return response.json();
}
