//! Main application state and logic

use crate::board::{BoardMessage, BoardView};
use crate::game::{ChessClock, GameResult, GameState, TimeControl};
use crate::styles::{EVAL_BAR_WIDTH, EVAL_BLACK, EVAL_WHITE, PANEL_WIDTH, SQUARE_SIZE};
use crate::tournament_view::{self, TournamentMessage, TournamentState};

use chess_core::{Color, Engine, Move};
use classical_engine::ClassicalEngine;
use iced::time;
use iced::widget::{
    button, column, container, horizontal_rule, pick_list, row, scrollable, slider, text,
    text_input, vertical_space,
};
use iced::{Element, Length, Subscription, Task, Theme};
use ml_engine::NeuralEngine;
use std::time::Duration;

/// Application tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Play,
    Tournament,
}

/// Player type for a game
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PlayerType {
    #[default]
    Human,
    Classical,
    Neural,
}

impl std::fmt::Display for PlayerType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlayerType::Human => write!(f, "Human"),
            PlayerType::Classical => write!(f, "Classical Engine"),
            PlayerType::Neural => write!(f, "Neural Engine"),
        }
    }
}

/// Time control presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimePreset {
    Bullet1_0,
    Bullet2_1,
    Blitz3_0,
    Blitz3_2,
    Blitz5_0,
    Blitz5_3,
    #[default]
    Rapid10_0,
    Rapid10_5,
    Rapid15_10,
    Classical30_0,
    Classical30_20,
    Unlimited,
    Custom,
}

impl TimePreset {
    pub fn to_time_control(self) -> TimeControl {
        match self {
            TimePreset::Bullet1_0 => TimeControl::new(1, 0),
            TimePreset::Bullet2_1 => TimeControl::new(2, 1),
            TimePreset::Blitz3_0 => TimeControl::new(3, 0),
            TimePreset::Blitz3_2 => TimeControl::new(3, 2),
            TimePreset::Blitz5_0 => TimeControl::new(5, 0),
            TimePreset::Blitz5_3 => TimeControl::new(5, 3),
            TimePreset::Rapid10_0 => TimeControl::new(10, 0),
            TimePreset::Rapid10_5 => TimeControl::new(10, 5),
            TimePreset::Rapid15_10 => TimeControl::new(15, 10),
            TimePreset::Classical30_0 => TimeControl::new(30, 0),
            TimePreset::Classical30_20 => TimeControl::new(30, 20),
            TimePreset::Unlimited => TimeControl::unlimited(),
            TimePreset::Custom => TimeControl::new(10, 0), // Default custom
        }
    }
}

impl std::fmt::Display for TimePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimePreset::Bullet1_0 => write!(f, "Bullet 1+0"),
            TimePreset::Bullet2_1 => write!(f, "Bullet 2+1"),
            TimePreset::Blitz3_0 => write!(f, "Blitz 3+0"),
            TimePreset::Blitz3_2 => write!(f, "Blitz 3+2"),
            TimePreset::Blitz5_0 => write!(f, "Blitz 5+0"),
            TimePreset::Blitz5_3 => write!(f, "Blitz 5+3"),
            TimePreset::Rapid10_0 => write!(f, "Rapid 10+0"),
            TimePreset::Rapid10_5 => write!(f, "Rapid 10+5"),
            TimePreset::Rapid15_10 => write!(f, "Rapid 15+10"),
            TimePreset::Classical30_0 => write!(f, "Classical 30+0"),
            TimePreset::Classical30_20 => write!(f, "Classical 30+20"),
            TimePreset::Unlimited => write!(f, "Unlimited"),
            TimePreset::Custom => write!(f, "Custom"),
        }
    }
}

/// Main application state
pub struct ChessApp {
    /// Current tab
    tab: Tab,
    /// Game state
    game: GameState,
    /// Board flipped?
    board_flipped: bool,
    /// White player type
    white_player: PlayerType,
    /// Black player type
    black_player: PlayerType,
    /// Search depth for engines
    engine_depth: u8,
    /// Tournament state
    tournament: TournamentState,
    /// Engine thinking in background
    engine_task_running: bool,
    /// Time control preset
    time_preset: TimePreset,
    /// Custom time (minutes)
    custom_time_mins: u64,
    /// Custom increment (seconds)
    custom_increment_secs: u64,
}

/// Application messages
#[derive(Debug, Clone)]
pub enum Message {
    // Navigation
    TabSelected(Tab),

    // Board interaction
    Board(BoardMessage),

    // Game controls
    NewGame,
    FlipBoard,
    WhitePlayerChanged(PlayerType),
    BlackPlayerChanged(PlayerType),
    DepthChanged(u8),

    // Time controls
    TimePresetChanged(TimePreset),
    CustomTimeChanged(String),
    CustomIncrementChanged(String),

    // Engine
    EngineMoveReady(Move, i32), // Move and evaluation

    // Clock tick
    ClockTick,

    // Tournament
    Tournament(TournamentMessage),
}

impl ChessApp {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                tab: Tab::Play,
                game: GameState::with_time_control(TimePreset::default().to_time_control()),
                board_flipped: false,
                white_player: PlayerType::Human,
                black_player: PlayerType::Classical,
                engine_depth: 4,
                tournament: TournamentState::new(),
                engine_task_running: false,
                time_preset: TimePreset::default(),
                custom_time_mins: 10,
                custom_increment_secs: 0,
            },
            Task::none(),
        )
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        // Tick the clock every 100ms when game is in progress and clock is running
        if self.game.result == GameResult::InProgress && self.game.clock.enabled {
            time::every(Duration::from_millis(100)).map(|_| Message::ClockTick)
        } else {
            Subscription::none()
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TabSelected(tab) => {
                self.tab = tab;
                if tab == Tab::Tournament {
                    self.tournament.refresh_elo();
                }
                Task::none()
            }

            Message::Board(BoardMessage::SquareClicked(sq)) => {
                // Only allow human moves
                let current_player = if self.game.position.side_to_move == Color::White {
                    &self.white_player
                } else {
                    &self.black_player
                };

                if *current_player == PlayerType::Human
                    && self.game.result == GameResult::InProgress
                    && !self.game.engine_thinking
                {
                    self.game.select_square(sq);

                    // Check if we need to trigger engine move
                    return self.maybe_trigger_engine_move();
                }
                Task::none()
            }

            Message::NewGame => {
                let time_control = if self.time_preset == TimePreset::Custom {
                    TimeControl::new(self.custom_time_mins, self.custom_increment_secs)
                } else {
                    self.time_preset.to_time_control()
                };
                self.game = GameState::with_time_control(time_control);
                self.engine_task_running = false;

                // Start the clock for the first player if not unlimited
                if self.game.clock.enabled {
                    self.game.clock.start(Color::White);
                }

                self.maybe_trigger_engine_move()
            }

            Message::FlipBoard => {
                self.board_flipped = !self.board_flipped;
                Task::none()
            }

            Message::WhitePlayerChanged(player) => {
                self.white_player = player;
                self.maybe_trigger_engine_move()
            }

            Message::BlackPlayerChanged(player) => {
                self.black_player = player;
                self.maybe_trigger_engine_move()
            }

            Message::DepthChanged(depth) => {
                self.engine_depth = depth;
                Task::none()
            }

            Message::TimePresetChanged(preset) => {
                self.time_preset = preset;
                Task::none()
            }

            Message::CustomTimeChanged(s) => {
                if let Ok(mins) = s.parse() {
                    self.custom_time_mins = mins;
                }
                Task::none()
            }

            Message::CustomIncrementChanged(s) => {
                if let Ok(secs) = s.parse() {
                    self.custom_increment_secs = secs;
                }
                Task::none()
            }

            Message::EngineMoveReady(mv, eval) => {
                self.game.engine_thinking = false;
                self.engine_task_running = false;
                self.game.set_evaluation(eval);

                if self.game.result == GameResult::InProgress {
                    self.game.apply_move(mv);
                    // Check if next player is also an engine
                    return self.maybe_trigger_engine_move();
                }
                Task::none()
            }

            Message::ClockTick => {
                // Check for timeout
                if self.game.clock.is_timeout(Color::White) {
                    self.game.result = GameResult::WhiteTimeout;
                } else if self.game.clock.is_timeout(Color::Black) {
                    self.game.result = GameResult::BlackTimeout;
                }
                Task::none()
            }

            Message::Tournament(msg) => self.handle_tournament_message(msg),
        }
    }

    /// Check if current player is an engine and trigger move calculation
    fn maybe_trigger_engine_move(&mut self) -> Task<Message> {
        if self.game.result != GameResult::InProgress || self.engine_task_running {
            return Task::none();
        }

        let current_player = if self.game.position.side_to_move == Color::White {
            &self.white_player
        } else {
            &self.black_player
        };

        if *current_player == PlayerType::Human {
            return Task::none();
        }

        // Start engine calculation
        self.engine_task_running = true;
        self.game.engine_thinking = true;

        let position = self.game.position.clone();
        let depth = self.engine_depth;
        let player_type = current_player.clone();
        let side_to_move = self.game.position.side_to_move;

        Task::perform(
            async move {
                // Run engine search in blocking task
                tokio::task::spawn_blocking(move || {
                    let mut engine: Box<dyn Engine> = match player_type {
                        PlayerType::Classical => Box::new(ClassicalEngine::new()),
                        PlayerType::Neural => Box::new(NeuralEngine::new()),
                        PlayerType::Human => unreachable!(),
                    };

                    let result = engine.search(&position, depth);
                    // Convert score to white's perspective (engine returns from side-to-move's view)
                    let score_from_white = if side_to_move == Color::White {
                        result.score
                    } else {
                        -result.score
                    };
                    (result.best_move, score_from_white)
                })
                .await
                .ok()
            },
            |result| {
                if let Some((Some(mv), score)) = result {
                    Message::EngineMoveReady(mv, score)
                } else {
                    // No move found (shouldn't happen in normal play)
                    Message::NewGame
                }
            },
        )
    }

    fn handle_tournament_message(&mut self, msg: TournamentMessage) -> Task<Message> {
        match msg {
            TournamentMessage::Engine1Selected(e) => {
                self.tournament.engine1 = Some(e);
            }
            TournamentMessage::Engine2Selected(e) => {
                self.tournament.engine2 = Some(e);
            }
            TournamentMessage::GamesChanged(s) => {
                if let Ok(n) = s.parse() {
                    self.tournament.num_games = n;
                }
            }
            TournamentMessage::DepthChanged(s) => {
                if let Ok(d) = s.parse() {
                    self.tournament.depth = d;
                }
            }
            TournamentMessage::StartTournament => {
                self.tournament.running = true;
                self.tournament.progress = 0;
                self.tournament.status = "Tournament running...".to_string();
                // TODO: Start actual tournament in background
            }
            TournamentMessage::StopTournament => {
                self.tournament.running = false;
                self.tournament.status = "Tournament stopped".to_string();
            }
            TournamentMessage::RefreshElo => {
                self.tournament.refresh_elo();
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let tabs = row![
            tab_button("Play", Tab::Play, self.tab),
            tab_button("Tournament", Tab::Tournament, self.tab),
        ]
        .spacing(5)
        .padding(10);

        let content: Element<'_, Message> = match self.tab {
            Tab::Play => self.play_view(),
            Tab::Tournament => {
                tournament_view::tournament_view(&self.tournament).map(Message::Tournament)
            }
        };

        column![tabs, horizontal_rule(2), content,].into()
    }

    /// Render the play/game view
    fn play_view(&self) -> Element<'_, Message> {
        // Evaluation bar
        let eval_bar = self.render_eval_bar();

        // Chess board
        let board = BoardView::new(&self.game, self.board_flipped)
            .view()
            .map(Message::Board);

        // Side panel with clocks
        let panel = self.control_panel();

        row![
            eval_bar,
            board,
            container(panel)
                .width(PANEL_WIDTH)
                .height(Length::Fill)
                .padding(15),
        ]
        .spacing(10)
        .padding(20)
        .into()
    }

    /// Render the evaluation bar
    fn render_eval_bar(&self) -> Element<'_, Message> {
        // Convert centipawns to a percentage (clamped)
        // +1000cp = 100% white, -1000cp = 100% black
        let eval = self.game.evaluation as f32;
        let white_percent = ((eval / 1000.0 + 1.0) / 2.0).clamp(0.05, 0.95);

        let board_height = SQUARE_SIZE * 8.0;
        let white_height = board_height * white_percent;
        let black_height = board_height * (1.0 - white_percent);

        // Flip if board is flipped
        let (top_height, top_color, bottom_height, bottom_color) = if self.board_flipped {
            (white_height, EVAL_WHITE, black_height, EVAL_BLACK)
        } else {
            (black_height, EVAL_BLACK, white_height, EVAL_WHITE)
        };

        let eval_text = if self.game.evaluation.abs() > 900 {
            if self.game.evaluation > 0 {
                "M".to_string()
            } else {
                "-M".to_string()
            }
        } else {
            format!("{:.1}", self.game.evaluation as f32 / 100.0)
        };

        // Text color should contrast with bottom_color
        let text_color = if bottom_color == EVAL_WHITE {
            EVAL_BLACK
        } else {
            EVAL_WHITE
        };

        column![
            container(text(""))
                .width(EVAL_BAR_WIDTH)
                .height(top_height)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(top_color)),
                    ..Default::default()
                }),
            container(text(eval_text).size(10).color(text_color))
                .width(EVAL_BAR_WIDTH)
                .height(bottom_height)
                .center_x(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::Background::Color(bottom_color)),
                    ..Default::default()
                }),
        ]
        .into()
    }

    /// Render a clock display
    fn render_clock(&self, color: Color, label: &'static str) -> Element<'static, Message> {
        let remaining = self.game.clock.remaining_time(color);
        let time_str = ChessClock::format_time(remaining);

        let is_active = self.game.clock.running_for == Some(color);
        let is_low = remaining.as_secs() < 30;

        let bg_color = if is_active {
            if is_low {
                iced::Color::from_rgb(0.8, 0.2, 0.2) // Red for low time
            } else {
                iced::Color::from_rgb(0.2, 0.5, 0.2) // Green for active
            }
        } else {
            iced::Color::from_rgb(0.2, 0.2, 0.2) // Dark for inactive
        };

        container(
            column![text(label).size(12), text(time_str).size(24),]
                .align_x(iced::Alignment::Center),
        )
        .width(Length::Fill)
        .padding(10)
        .style(move |_| container::Style {
            background: Some(iced::Background::Color(bg_color)),
            border: iced::Border {
                radius: 5.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
    }

    /// Render the control panel
    fn control_panel(&self) -> Element<'_, Message> {
        let player_types = vec![PlayerType::Human, PlayerType::Classical, PlayerType::Neural];

        let time_presets = vec![
            TimePreset::Bullet1_0,
            TimePreset::Bullet2_1,
            TimePreset::Blitz3_0,
            TimePreset::Blitz3_2,
            TimePreset::Blitz5_0,
            TimePreset::Blitz5_3,
            TimePreset::Rapid10_0,
            TimePreset::Rapid10_5,
            TimePreset::Rapid15_10,
            TimePreset::Classical30_0,
            TimePreset::Classical30_20,
            TimePreset::Unlimited,
            TimePreset::Custom,
        ];

        // Clock displays - show opponent clock on top (from player's perspective)
        let (top_clock, bottom_clock) = if self.board_flipped {
            (
                self.render_clock(Color::White, "White"),
                self.render_clock(Color::Black, "Black"),
            )
        } else {
            (
                self.render_clock(Color::Black, "Black"),
                self.render_clock(Color::White, "White"),
            )
        };

        // Game controls
        let new_game_btn = button(text("New Game"))
            .on_press(Message::NewGame)
            .style(button::primary)
            .width(Length::Fill);

        let flip_btn = button(text("Flip Board"))
            .on_press(Message::FlipBoard)
            .style(button::secondary)
            .width(Length::Fill);

        // Player selection
        let white_picker = pick_list(
            player_types.clone(),
            Some(self.white_player.clone()),
            Message::WhitePlayerChanged,
        )
        .width(Length::Fill);

        let black_picker = pick_list(
            player_types,
            Some(self.black_player.clone()),
            Message::BlackPlayerChanged,
        )
        .width(Length::Fill);

        // Time control
        let time_picker = pick_list(
            time_presets,
            Some(self.time_preset),
            Message::TimePresetChanged,
        )
        .width(Length::Fill);

        // Custom time inputs (only show when custom is selected)
        let custom_time_controls: Element<'_, Message> = if self.time_preset == TimePreset::Custom {
            row![
                column![
                    text("Minutes").size(12),
                    text_input("10", &self.custom_time_mins.to_string())
                        .on_input(Message::CustomTimeChanged)
                        .width(60),
                ],
                column![
                    text("Increment").size(12),
                    text_input("0", &self.custom_increment_secs.to_string())
                        .on_input(Message::CustomIncrementChanged)
                        .width(60),
                ],
            ]
            .spacing(10)
            .into()
        } else {
            text("").into()
        };

        // Depth slider
        let depth_slider = row![
            text(format!("Depth: {}", self.engine_depth)).size(14),
            slider(1..=10, self.engine_depth, Message::DepthChanged).width(Length::Fill),
        ]
        .spacing(10);

        // Status
        let status = match self.game.result {
            GameResult::InProgress => {
                if self.game.engine_thinking {
                    "Engine thinking...".to_string()
                } else {
                    let side = if self.game.position.side_to_move == Color::White {
                        "White"
                    } else {
                        "Black"
                    };
                    format!("{} to move", side)
                }
            }
            GameResult::WhiteWins => "Checkmate! White wins".to_string(),
            GameResult::BlackWins => "Checkmate! Black wins".to_string(),
            GameResult::Draw => "Draw".to_string(),
            GameResult::WhiteTimeout => "White ran out of time! Black wins".to_string(),
            GameResult::BlackTimeout => "Black ran out of time! White wins".to_string(),
        };

        // Move history
        let moves_title = text("Moves").size(16);
        let mut moves_list = column![].spacing(2);

        for (i, chunk) in self.game.moves.chunks(2).enumerate() {
            let move_num = i + 1;
            let white_move = &chunk[0].san;
            let black_move = chunk.get(1).map(|m| m.san.as_str()).unwrap_or("");

            moves_list = moves_list
                .push(text(format!("{}. {} {}", move_num, white_move, black_move)).size(13));
        }

        let moves_scroll = scrollable(moves_list).height(Length::Fill);

        let status_text = text(status).size(14);

        column![
            top_clock,
            vertical_space().height(10),
            new_game_btn,
            flip_btn,
            vertical_space().height(10),
            text("Time Control").size(14),
            time_picker,
            custom_time_controls,
            vertical_space().height(10),
            text("White Player").size(14),
            white_picker,
            text("Black Player").size(14),
            black_picker,
            vertical_space().height(10),
            depth_slider,
            vertical_space().height(10),
            horizontal_rule(1),
            vertical_space().height(5),
            status_text,
            vertical_space().height(10),
            bottom_clock,
            vertical_space().height(10),
            horizontal_rule(1),
            vertical_space().height(5),
            moves_title,
            moves_scroll,
        ]
        .spacing(3)
        .into()
    }
}

/// Create a tab button
fn tab_button(label: &str, tab: Tab, current: Tab) -> Element<'static, Message> {
    let is_active = tab == current;

    button(text(label.to_string()))
        .on_press(Message::TabSelected(tab))
        .style(if is_active {
            button::primary
        } else {
            button::secondary
        })
        .into()
}
