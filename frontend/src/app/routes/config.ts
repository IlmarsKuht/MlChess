export type WorkspaceView =
  | "overview"
  | "setup"
  | "live-duel"
  | "play-engine"
  | "events"
  | "tournaments"
  | "replay";

export const workspaceViews: Array<{ id: WorkspaceView; label: string; detail: string; path: string }> = [
  { id: "overview", label: "Home", detail: "Live standings and recent games", path: "/" },
  { id: "setup", label: "Engines", detail: "Available engines and formats", path: "/setup" },
  { id: "live-duel", label: "Live Duel", detail: "Start duels and see live matches", path: "/live-duel" },
  { id: "play-engine", label: "Play vs Engine", detail: "Launch and play human games", path: "/play-engine" },
  { id: "events", label: "Events", detail: "Start backend-defined runs", path: "/events" },
  { id: "tournaments", label: "Tournaments", detail: "Bracket-like matchup map", path: "/tournaments" },
  { id: "replay", label: "Replay", detail: "Boards, moves and results", path: "/replay" }
];
