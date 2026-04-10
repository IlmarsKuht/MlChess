use std::collections::BTreeSet;

use chrono::Utc;
use cozy_chess::Board;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::{OpeningPosition, OpeningSourceKind, OpeningSuite, Variant};

#[derive(Debug, Error)]
pub enum OpeningImportError {
    #[error("opening source was empty")]
    Empty,
    #[error("invalid FEN on line {line}: {message}")]
    InvalidFen { line: usize, message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpeningImportRequest {
    pub registry_key: Option<String>,
    pub name: String,
    pub description: Option<String>,
    pub variant: Variant,
    pub text: String,
    pub source_kind: OpeningSourceKind,
    pub starter: bool,
}

pub fn import_opening_suite(
    input: OpeningImportRequest,
) -> Result<OpeningSuite, OpeningImportError> {
    let normalized_text = match input.source_kind {
        OpeningSourceKind::Starter | OpeningSourceKind::FenList => input.text.clone(),
        OpeningSourceKind::PgnImport => extract_fens_from_pgn(&input.text),
    };

    let suite_id = Uuid::new_v4();
    let mut seen = BTreeSet::new();
    let mut positions = Vec::new();

    for (line_idx, raw_line) in normalized_text.lines().enumerate() {
        let fen = raw_line.trim();
        if fen.is_empty() {
            continue;
        }
        let board = fen
            .parse::<Board>()
            .map_err(|err| OpeningImportError::InvalidFen {
                line: line_idx + 1,
                message: err.to_string(),
            })?;
        let normalized = board.to_string();
        if seen.insert(normalized.clone()) {
            positions.push(OpeningPosition {
                id: Uuid::new_v4(),
                suite_id,
                label: format!("Position {}", positions.len() + 1),
                fen: normalized,
                variant: input.variant,
            });
        }
    }

    if positions.is_empty() {
        return Err(OpeningImportError::Empty);
    }

    Ok(OpeningSuite {
        id: suite_id,
        registry_key: input.registry_key,
        name: input.name,
        description: input.description,
        source_kind: input.source_kind,
        source_text: Some(input.text),
        active: true,
        starter: input.starter,
        positions,
        created_at: Utc::now(),
    })
}

pub fn starter_suite() -> OpeningSuite {
    import_opening_suite(OpeningImportRequest {
        registry_key: Some("starter-benchmark-suite".to_string()),
        name: "Starter Benchmark Suite".to_string(),
        description: Some("Small opening suite for local benchmarking.".to_string()),
        variant: Variant::Standard,
        text: [
            "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
            "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
        ]
        .join("\n"),
        source_kind: OpeningSourceKind::Starter,
        starter: true,
    })
    .expect("starter suite should be valid")
}

fn extract_fens_from_pgn(text: &str) -> String {
    let mut fens = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("[FEN \"") {
            if let Some(end) = value.find("\"]") {
                fens.push(value[..end].to_string());
            }
        }
    }
    fens.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_suite_deduplicates_positions() {
        let suite = import_opening_suite(OpeningImportRequest {
            registry_key: None,
            name: "test".to_string(),
            description: None,
            variant: Variant::Standard,
            text: [
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
                "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
            ]
            .join("\n"),
            source_kind: OpeningSourceKind::FenList,
            starter: false,
        })
        .unwrap();

        assert_eq!(suite.positions.len(), 1);
    }

    #[test]
    fn pgn_import_reads_fen_tags() {
        let suite = import_opening_suite(OpeningImportRequest {
            registry_key: None,
            name: "pgn".to_string(),
            description: None,
            variant: Variant::Standard,
            text: "[Event \"Test\"]\n[FEN \"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1\"]".to_string(),
            source_kind: OpeningSourceKind::PgnImport,
            starter: false,
        })
        .unwrap();

        assert_eq!(suite.positions.len(), 1);
    }
}
