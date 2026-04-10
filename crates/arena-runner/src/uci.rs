use std::{path::PathBuf, process::Stdio, time::Duration};

use anyhow::{Context, Result, anyhow};
use arena_core::{AgentVersion, GameLogEntry, Variant};
use async_trait::async_trait;
use cozy_chess::Board;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
    time::timeout,
};
use tracing::warn;

use crate::match_runner::AgentAdapter;

pub(crate) struct UciAgentAdapter {
    version: AgentVersion,
    session: Option<UciSession>,
}

impl UciAgentAdapter {
    pub(crate) fn new(version: AgentVersion) -> Self {
        Self {
            version,
            session: None,
        }
    }
}

#[async_trait]
impl AgentAdapter for UciAgentAdapter {
    async fn prepare(&mut self, variant: Variant, logs: &mut Vec<GameLogEntry>) -> Result<()> {
        let mut session = UciSession::spawn(&self.version).await?;
        session.handshake(variant, logs).await?;
        self.session = Some(session);
        Ok(())
    }

    async fn begin_game(&mut self, logs: &mut Vec<GameLogEntry>) -> Result<()> {
        self.session
            .as_mut()
            .ok_or_else(|| anyhow!("session not prepared"))?
            .new_game(logs)
            .await
    }

    async fn choose_move(
        &mut self,
        board: &Board,
        start_fen: &str,
        moves: &[String],
        movetime_ms: u64,
        logs: &mut Vec<GameLogEntry>,
    ) -> Result<String> {
        self.session
            .as_mut()
            .ok_or_else(|| anyhow!("session not prepared"))?
            .best_move(board, start_fen, moves, movetime_ms, logs)
            .await
    }

    async fn shutdown(&mut self, logs: &mut Vec<GameLogEntry>) -> Result<()> {
        if let Some(session) = self.session.as_mut() {
            session.shutdown(logs).await?;
        }
        self.session = None;
        Ok(())
    }
}

struct UciSession {
    child: Child,
    stdin: ChildStdin,
    stdout: Lines<BufReader<ChildStdout>>,
}

impl UciSession {
    async fn spawn(version: &AgentVersion) -> Result<Self> {
        let mut command = Command::new(&version.executable_path);
        command.args(&version.args);
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(cwd) = &version.working_directory {
            command.current_dir(PathBuf::from(cwd));
        }
        command.envs(&version.env);
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start {}", version.executable_path))?;
        let stdin = child.stdin.take().context("missing child stdin")?;
        let stdout = child.stdout.take().context("missing child stdout")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout).lines(),
        })
    }

    async fn handshake(&mut self, variant: Variant, logs: &mut Vec<GameLogEntry>) -> Result<()> {
        self.send("uci", logs).await?;
        self.read_until("uciok", logs).await?;
        if variant.is_chess960() {
            self.send("setoption name UCI_Chess960 value true", logs)
                .await?;
        }
        self.send("isready", logs).await?;
        self.read_until("readyok", logs).await?;
        Ok(())
    }

    async fn new_game(&mut self, logs: &mut Vec<GameLogEntry>) -> Result<()> {
        self.send("ucinewgame", logs).await?;
        self.send("isready", logs).await?;
        self.read_until("readyok", logs).await?;
        Ok(())
    }

    async fn best_move(
        &mut self,
        _board: &Board,
        start_fen: &str,
        moves: &[String],
        movetime_ms: u64,
        logs: &mut Vec<GameLogEntry>,
    ) -> Result<String> {
        let position = if moves.is_empty() {
            format!("position fen {start_fen}")
        } else {
            format!("position fen {start_fen} moves {}", moves.join(" "))
        };
        self.send(&position, logs).await?;
        self.send(&format!("go movetime {movetime_ms}"), logs)
            .await?;

        loop {
            let line = self
                .read_line(Duration::from_millis(movetime_ms + 2_000), logs)
                .await?;
            if let Some(bestmove) = line.strip_prefix("bestmove ") {
                let token = bestmove.split_whitespace().next().unwrap_or("0000");
                return Ok(token.to_string());
            }
        }
    }

    async fn shutdown(&mut self, logs: &mut Vec<GameLogEntry>) -> Result<()> {
        self.send("quit", logs).await.ok();
        if let Err(err) = self.child.kill().await {
            warn!("failed to kill engine process cleanly: {err}");
        }
        Ok(())
    }

    async fn send(&mut self, command: &str, logs: &mut Vec<GameLogEntry>) -> Result<()> {
        logs.push(GameLogEntry {
            timestamp_ms: 0,
            level: "debug".to_string(),
            source: "runner->engine".to_string(),
            message: command.to_string(),
        });
        self.stdin.write_all(command.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn read_until(&mut self, needle: &str, logs: &mut Vec<GameLogEntry>) -> Result<()> {
        loop {
            let line = self.read_line(Duration::from_secs(5), logs).await?;
            if line == needle {
                return Ok(());
            }
        }
    }

    async fn read_line(&mut self, wait: Duration, logs: &mut Vec<GameLogEntry>) -> Result<String> {
        let line = timeout(wait, self.stdout.next_line())
            .await
            .context("timed out waiting for engine output")??;
        let line = line.context("engine process ended unexpectedly")?;
        logs.push(GameLogEntry {
            timestamp_ms: 0,
            level: "debug".to_string(),
            source: "engine->runner".to_string(),
            message: line.clone(),
        });
        Ok(line)
    }
}
