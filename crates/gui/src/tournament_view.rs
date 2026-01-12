//! Tournament view and management

use crate::board::render_static_board;
use chess_core::Position;
use iced::widget::{
    button, column, horizontal_rule, pick_list, row, scrollable, text, text_input, vertical_space,
};
use iced::{Element, Length};
use tournament::EloTracker;

/// Available engines for tournament
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineOption {
    pub id: String,
    pub display_name: String,
}

impl std::fmt::Display for EngineOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name)
    }
}

impl Default for EngineOption {
    fn default() -> Self {
        Self {
            id: "classical".to_string(),
            display_name: "Classical".to_string(),
        }
    }
}

/// Tournament configuration state
#[derive(Debug, Clone)]
pub struct TournamentState {
    /// Available engines
    pub engines: Vec<EngineOption>,
    /// Selected engine 1
    pub engine1: Option<EngineOption>,
    /// Selected engine 2
    pub engine2: Option<EngineOption>,
    /// Number of games
    pub num_games: u32,
    /// Search depth
    pub depth: u8,
    /// Time limit per move in milliseconds (0 = no limit)
    pub time_per_move_ms: u64,
    /// Is tournament running?
    pub running: bool,
    /// Current progress (games completed)
    pub progress: u32,
    /// Elo tracker
    pub elo_tracker: EloTracker,
    /// Status message
    pub status: String,
    /// Watch tournament games live
    pub watch_live: bool,
    /// Current live game position (when watching)
    pub live_position: Position,
    /// Last move in live game (for highlighting)
    pub live_last_move: Option<(u8, u8)>,
    /// Current game info (e.g., "Game 3/10: Classical vs Neural")
    pub live_game_info: String,
}

impl Default for TournamentState {
    fn default() -> Self {
        Self::new()
    }
}

impl TournamentState {
    pub fn new() -> Self {
        let engines = vec![
            EngineOption {
                id: "classical".to_string(),
                display_name: "Classical (Alpha-Beta)".to_string(),
            },
            EngineOption {
                id: "neural".to_string(),
                display_name: "Neural (Random fallback)".to_string(),
            },
            EngineOption {
                id: "neural:v001".to_string(),
                display_name: "Neural v001".to_string(),
            },
        ];

        let elo_tracker = EloTracker::load("tournament_elo.json").unwrap_or_default();

        Self {
            engine1: Some(engines[0].clone()),
            engine2: Some(engines[1].clone()),
            engines,
            num_games: 10,
            depth: 4,
            time_per_move_ms: 0,
            running: false,
            progress: 0,
            elo_tracker,
            status: "Ready to start tournament".to_string(),
            watch_live: false,
            live_position: Position::startpos(),
            live_last_move: None,
            live_game_info: String::new(),
        }
    }

    pub fn refresh_elo(&mut self) {
        self.elo_tracker = EloTracker::load("tournament_elo.json").unwrap_or_default();
    }
}

/// Messages for tournament view
#[derive(Debug, Clone)]
pub enum TournamentMessage {
    Engine1Selected(EngineOption),
    Engine2Selected(EngineOption),
    GamesChanged(String),
    DepthChanged(String),
    TimePerMoveChanged(String),
    StartTournament,
    StopTournament,
    RefreshElo,
    ToggleWatchLive,
    /// Position update from running tournament
    PositionUpdate {
        position: Box<Position>,
        last_move: Option<(u8, u8)>,
        game_info: String,
    },
    /// A game in the tournament finished
    GameFinished {
        game_num: u32,
        result: String,
    },
    /// Tournament completed
    TournamentFinished,
}

/// Render the tournament view
pub fn tournament_view(state: &TournamentState) -> Element<'_, TournamentMessage> {
    let title = text("Tournament").size(28);

    // Engine selection
    let engine1_picker = pick_list(
        state.engines.clone(),
        state.engine1.clone(),
        TournamentMessage::Engine1Selected,
    )
    .width(200)
    .placeholder("Select Engine 1");

    let engine2_picker = pick_list(
        state.engines.clone(),
        state.engine2.clone(),
        TournamentMessage::Engine2Selected,
    )
    .width(200)
    .placeholder("Select Engine 2");

    let engine_row = row![
        column![text("Engine 1").size(14), engine1_picker,].spacing(5),
        text("vs").size(20),
        column![text("Engine 2").size(14), engine2_picker,].spacing(5),
    ]
    .spacing(20)
    .align_y(iced::Alignment::Center);

    // Settings
    let games_input = text_input("10", &state.num_games.to_string())
        .on_input(TournamentMessage::GamesChanged)
        .width(80);

    let depth_input = text_input("4", &state.depth.to_string())
        .on_input(TournamentMessage::DepthChanged)
        .width(80);

    let time_display = if state.time_per_move_ms == 0 {
        String::new()
    } else {
        state.time_per_move_ms.to_string()
    };
    let time_input = text_input("∞", &time_display)
        .on_input(TournamentMessage::TimePerMoveChanged)
        .width(80);

    let settings_row = row![
        column![text("Games").size(14), games_input,].spacing(5),
        column![text("Depth").size(14), depth_input,].spacing(5),
        column![text("Time/move (ms)").size(14), time_input,].spacing(5),
    ]
    .spacing(20);

    // Start/Stop button
    let action_button = if state.running {
        button(text("Stop Tournament"))
            .on_press(TournamentMessage::StopTournament)
            .style(button::danger)
    } else {
        button(text("Start Tournament"))
            .on_press(TournamentMessage::StartTournament)
            .style(button::success)
    };

    // Watch Live toggle button
    let watch_live_button = if state.watch_live {
        button(text("👁 Watching Live"))
            .on_press(TournamentMessage::ToggleWatchLive)
            .style(button::primary)
    } else {
        button(text("👁 Watch Live"))
            .on_press(TournamentMessage::ToggleWatchLive)
            .style(button::secondary)
    };

    let action_row = row![action_button, watch_live_button].spacing(10);

    // Live board view (when watching and tournament is running)
    let live_board: Element<'_, TournamentMessage> =
        if state.watch_live && state.running && !state.live_game_info.is_empty() {
            let board: Element<'static, TournamentMessage> =
                render_static_board(&state.live_position, state.live_last_move, false);
            column![
                text(&state.live_game_info).size(16),
                vertical_space().height(10),
                board,
            ]
            .spacing(5)
            .into()
        } else if state.watch_live && state.running {
            text("Waiting for game to start...").size(14).into()
        } else {
            vertical_space().height(0).into()
        };

    // Progress
    let progress_text = if state.running {
        format!("Progress: {}/{} games", state.progress, state.num_games)
    } else {
        state.status.clone()
    };

    // Elo Leaderboard
    let leaderboard_title = text("Elo Leaderboard").size(20);
    let refresh_btn = button(text("Refresh"))
        .on_press(TournamentMessage::RefreshElo)
        .style(button::secondary);

    let leaderboard_header = row![
        text("Engine").width(Length::FillPortion(3)),
        text("Elo").width(Length::FillPortion(1)),
        text("Games").width(Length::FillPortion(1)),
    ]
    .spacing(10);

    let mut leaderboard_rows = column![leaderboard_header, horizontal_rule(1)].spacing(5);

    for (name, rating, games) in state.elo_tracker.leaderboard() {
        let row_widget = row![
            text(name).width(Length::FillPortion(3)),
            text(format!("{:.0}", rating)).width(Length::FillPortion(1)),
            text(format!("{}", games)).width(Length::FillPortion(1)),
        ]
        .spacing(10);
        leaderboard_rows = leaderboard_rows.push(row_widget);
    }

    let leaderboard = scrollable(leaderboard_rows).height(Length::Fill);

    let progress_widget = text(progress_text).size(14);

    // Layout
    column![
        title,
        vertical_space().height(20),
        engine_row,
        vertical_space().height(15),
        settings_row,
        vertical_space().height(15),
        action_row,
        vertical_space().height(10),
        progress_widget,
        vertical_space().height(10),
        live_board,
        vertical_space().height(20),
        horizontal_rule(2),
        vertical_space().height(20),
        row![leaderboard_title, horizontal_space(), refresh_btn].spacing(10),
        vertical_space().height(10),
        leaderboard,
    ]
    .spacing(5)
    .padding(20)
    .into()
}

fn horizontal_space() -> iced::widget::Space {
    iced::widget::Space::with_width(Length::Fill)
}
