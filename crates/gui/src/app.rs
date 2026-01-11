//! Main application state and logic

use crate::board::{BoardMessage, BoardView};
use crate::game::{GameResult, GameState};
use crate::styles::PANEL_WIDTH;
use crate::tournament_view::{self, TournamentMessage, TournamentState};

use chess_core::{Color, Engine, Move};
use classical_engine::ClassicalEngine;
use iced::widget::{
    button, column, container, horizontal_rule, pick_list, row, scrollable,
    text, vertical_space,
};
use iced::{Element, Length, Subscription, Task, Theme};
use ml_engine::NeuralEngine;

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

    // Engine
    EngineMoveReady(Move),

    // Tournament
    Tournament(TournamentMessage),
}

impl ChessApp {
    pub fn new() -> (Self, Task<Message>) {
        (
            Self {
                tab: Tab::Play,
                game: GameState::new(),
                board_flipped: false,
                white_player: PlayerType::Human,
                black_player: PlayerType::Classical,
                engine_depth: 4,
                tournament: TournamentState::new(),
                engine_task_running: false,
            },
            Task::none(),
        )
    }

    pub fn theme(&self) -> Theme {
        Theme::Dark
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
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
                self.game.reset();
                self.engine_task_running = false;
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

            Message::EngineMoveReady(mv) => {
                self.game.engine_thinking = false;
                self.engine_task_running = false;
                
                if self.game.result == GameResult::InProgress {
                    self.game.apply_move(mv);
                    // Check if next player is also an engine
                    return self.maybe_trigger_engine_move();
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
                    result.best_move
                })
                .await
                .ok()
                .flatten()
            },
            |mv| {
                if let Some(mv) = mv {
                    Message::EngineMoveReady(mv)
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
            Tab::Tournament => tournament_view::tournament_view(&self.tournament)
                .map(Message::Tournament),
        };

        column![
            tabs,
            horizontal_rule(2),
            content,
        ]
        .into()
    }

    /// Render the play/game view
    fn play_view(&self) -> Element<'_, Message> {
        // Chess board
        let board = BoardView::new(&self.game, self.board_flipped)
            .view()
            .map(Message::Board);

        // Side panel
        let panel = self.control_panel();

        row![
            board,
            container(panel)
                .width(PANEL_WIDTH)
                .height(Length::Fill)
                .padding(15),
        ]
        .spacing(20)
        .padding(20)
        .into()
    }

    /// Render the control panel
    fn control_panel(&self) -> Element<'_, Message> {
        let player_types = vec![
            PlayerType::Human,
            PlayerType::Classical,
            PlayerType::Neural,
        ];

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

        // Depth setting
        let depth_text = text(format!("Engine Depth: {}", self.engine_depth)).size(14);

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
        };

        // Move history
        let moves_title = text("Moves").size(16);
        let mut moves_list = column![].spacing(2);
        
        for (i, chunk) in self.game.moves.chunks(2).enumerate() {
            let move_num = i + 1;
            let white_move = &chunk[0].san;
            let black_move = chunk.get(1).map(|m| m.san.as_str()).unwrap_or("");
            
            moves_list = moves_list.push(
                text(format!("{}. {} {}", move_num, white_move, black_move)).size(13)
            );
        }

        let moves_scroll = scrollable(moves_list)
            .height(Length::Fill);

        let status_text = text(status).size(16);

        column![
            new_game_btn,
            flip_btn,
            vertical_space().height(20),
            text("White Player").size(14),
            white_picker,
            vertical_space().height(10),
            text("Black Player").size(14),
            black_picker,
            vertical_space().height(15),
            depth_text,
            vertical_space().height(20),
            horizontal_rule(1),
            vertical_space().height(10),
            status_text,
            vertical_space().height(20),
            horizontal_rule(1),
            vertical_space().height(10),
            moves_title,
            moves_scroll,
        ]
        .spacing(5)
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
