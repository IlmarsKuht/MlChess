export type Variant = "standard" | "chess960";
export type TournamentKind = "round_robin" | "ladder";
export type WorkspaceView =
  | "overview"
  | "setup"
  | "live_duel"
  | "play_engine"
  | "events"
  | "tournament"
  | "replay";
export type GameResult = "white_win" | "black_win" | "draw";
export type LiveSide = "white" | "black";
export type ProtocolLiveSide = "white" | "black" | "none";
export type LiveStatus = "running" | "finished" | "aborted";
export type MatchWatchState = "live" | "replay" | "unavailable";
export type LiveResult = "white_win" | "black_win" | "draw" | "none";
export type GameTermination =
  | "checkmate"
  | "stalemate"
  | "fifty_move_rule"
  | "repetition"
  | "insufficient_material"
  | "timeout"
  | "resignation"
  | "abort"
  | "illegal_move"
  | "move_limit"
  | "engine_failure"
  | "unknown"
  | "none";
export type RouteState =
  | { page: "app"; view: WorkspaceView }
  | { page: "engine"; engineId: string }
  | { page: "watch"; matchId: string };

export interface Participant {
  kind: "engine_version" | "human_player";
  id: string;
  display_name: string;
}

export interface Agent {
  id: string;
  registry_key?: string | null;
  name: string;
  tags: string[];
  notes?: string | null;
  documentation?: string | null;
}

export interface AgentVersion {
  id: string;
  registry_key?: string | null;
  agent_id: string;
  version: string;
  active: boolean;
  executable_path: string;
  working_directory?: string | null;
  args: string[];
  declared_name?: string | null;
  tags: string[];
  notes?: string | null;
  documentation?: string | null;
}

export interface TimeControl {
  initial_ms: number;
  increment_ms: number;
}

export interface FairnessConfig {
  paired_games: boolean;
  swap_colors: boolean;
  opening_suite_id?: string | null;
  opening_seed?: number | null;
}

export interface BenchmarkPool {
  id: string;
  registry_key?: string | null;
  name: string;
  description?: string | null;
  variant: Variant;
  time_control: TimeControl;
  fairness: FairnessConfig;
}

export interface EventPreset {
  id: string;
  registry_key?: string | null;
  name: string;
  kind: TournamentKind;
  pool_id: string;
  selection_mode: "all_active_engines";
  worker_count: number;
  games_per_pairing: number;
  active: boolean;
}

export interface Tournament {
  id: string;
  name: string;
  kind: TournamentKind;
  pool_id: string;
  participant_version_ids: string[];
  worker_count: number;
  games_per_pairing: number;
  status: string;
  started_at?: string | null;
  completed_at?: string | null;
}

export interface MatchSeries {
  id: string;
  tournament_id: string;
  pool_id: string;
  round_index: number;
  white_version_id: string;
  black_version_id: string;
  opening_id?: string | null;
  game_index: number;
  status: string;
  watch_state: MatchWatchState;
  game_id?: string | null;
  created_at: string;
  white_participant: Participant;
  black_participant: Participant;
  interactive: boolean;
}

export interface LeaderboardEntry {
  participant: Participant;
  rating: number;
  games_played: number;
  wins: number;
  draws: number;
  losses: number;
}

export interface GameRecord {
  id: string;
  tournament_id: string;
  match_id: string;
  pool_id: string;
  variant: Variant;
  white_version_id: string;
  black_version_id: string;
  result: GameResult;
  termination: GameTermination;
  start_fen: string;
  pgn: string;
  moves_uci: string[];
  white_time_left_ms: number;
  black_time_left_ms: number;
  started_at: string;
  completed_at: string;
  white_participant: Participant;
  black_participant: Participant;
}

export interface ReplayPayload {
  id: string;
  variant: Variant;
  start_fen: string;
  pgn: string;
  moves_uci: string[];
  result: GameResult;
  termination: GameTermination;
}

export interface LiveMatchSnapshot {
  match_id: string;
  protocol_version: number;
  event_type: "snapshot";
  seq: number;
  server_now_unix_ms: number;
  status: LiveStatus;
  result: LiveResult;
  termination: GameTermination;
  fen: string;
  moves: string[];
  white_remaining_ms: number;
  black_remaining_ms: number;
  side_to_move: ProtocolLiveSide;
  turn_started_server_unix_ms: number;
}

export interface MoveCommittedEvent {
  protocol_version: number;
  event_type: "move_committed";
  match_id: string;
  seq: number;
  server_now_unix_ms: number;
  status: LiveStatus;
  move_uci: string;
  fen: string;
  moves: string[];
  white_remaining_ms: number;
  black_remaining_ms: number;
  side_to_move: ProtocolLiveSide;
  turn_started_server_unix_ms: number;
}

export interface ClockSyncEvent {
  protocol_version: number;
  event_type: "clock_sync";
  match_id: string;
  seq: number;
  server_now_unix_ms: number;
  status: LiveStatus;
  white_remaining_ms: number;
  black_remaining_ms: number;
  side_to_move: ProtocolLiveSide;
  turn_started_server_unix_ms: number;
}

export interface GameFinishedEvent {
  protocol_version: number;
  event_type: "game_finished";
  match_id: string;
  seq: number;
  server_now_unix_ms: number;
  status: LiveStatus;
  result: LiveResult;
  termination: GameTermination;
  fen: string;
  moves: string[];
  white_remaining_ms: number;
  black_remaining_ms: number;
  side_to_move: ProtocolLiveSide;
  turn_started_server_unix_ms: number;
}

export type LiveProtocolEvent = LiveMatchSnapshot | MoveCommittedEvent | ClockSyncEvent | GameFinishedEvent;

export interface LiveIntentAck {
  message_type: "intent_ack";
  match_id: string;
  intent_id: string;
  client_action_id?: string;
  ws_connection_id?: string;
  request_id?: string;
  ack: "accepted" | "duplicate";
}

export interface LiveErrorMessage {
  message_type: "error";
  error: string;
  request_id?: string;
  client_action_id?: string;
  ws_connection_id?: string;
}

export interface LiveSubscribeMessage {
  message_type: "subscribe";
  last_seq?: number;
  ws_connection_id?: string;
}

export interface LiveSubmitMoveMessage {
  message_type: "submit_move";
  intent_id?: string;
  client_action_id?: string;
  ws_connection_id?: string;
  move_uci: string;
}

export type LiveWsServerMessage = LiveProtocolEvent | LiveIntentAck | LiveErrorMessage;
export type LiveWsClientMessage = LiveSubscribeMessage | LiveSubmitMoveMessage;

export interface HumanPlayerProfile {
  id: string;
  name: string;
  rating: number;
  games_played: number;
  wins: number;
  draws: number;
  losses: number;
}

export interface BoardMoveMarker {
  square: string;
  kind: "quiet" | "capture";
}
