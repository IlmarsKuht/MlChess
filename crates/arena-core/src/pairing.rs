use uuid::Uuid;

use crate::{TournamentKind, TournamentStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pairing {
    pub round_index: u32,
    pub white_version_id: Uuid,
    pub black_version_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledTournament {
    pub kind: TournamentKind,
    pub status: TournamentStatus,
    pub pairings: Vec<Pairing>,
}

pub fn build_round_robin_pairings(
    participants: &[Uuid],
    games_per_pairing: u16,
    swap_colors: bool,
) -> Vec<Pairing> {
    if participants.len() < 2 {
        return Vec::new();
    }

    let mut ids = participants.to_vec();
    let ghost = Uuid::nil();
    if ids.len() % 2 == 1 {
        ids.push(ghost);
    }

    let n = ids.len();
    let rounds = n - 1;
    let half = n / 2;
    let mut rotation = ids;
    let mut pairings = Vec::new();

    for round in 0..rounds {
        for index in 0..half {
            let left = rotation[index];
            let right = rotation[n - 1 - index];
            if left == ghost || right == ghost {
                continue;
            }

            let even = (round + index) % 2 == 0;
            let (white, black) = if even { (left, right) } else { (right, left) };
            pairings.push(Pairing {
                round_index: round as u32,
                white_version_id: white,
                black_version_id: black,
            });

            if swap_colors {
                pairings.push(Pairing {
                    round_index: round as u32,
                    white_version_id: black,
                    black_version_id: white,
                });
            }

            for _ in 1..games_per_pairing {
                pairings.push(Pairing {
                    round_index: round as u32,
                    white_version_id: white,
                    black_version_id: black,
                });
            }
        }

        rotation[1..].rotate_right(1);
    }

    pairings
}

pub fn build_ladder_pairings(participants_by_rating: &[Uuid], rounds: u16) -> Vec<Pairing> {
    let mut pairings = Vec::new();
    if participants_by_rating.len() < 2 {
        return pairings;
    }

    for round in 0..rounds {
        for window in participants_by_rating.windows(2) {
            pairings.push(Pairing {
                round_index: round as u32,
                white_version_id: window[0],
                black_version_id: window[1],
            });
        }
    }

    pairings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_robin_pairs_everyone() {
        let participants = vec![
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        ];
        let pairings = build_round_robin_pairings(&participants, 1, false);
        assert_eq!(pairings.len(), 6);
    }

    #[test]
    fn round_robin_swaps_colors() {
        let participants = vec![Uuid::new_v4(), Uuid::new_v4()];
        let pairings = build_round_robin_pairings(&participants, 1, true);
        assert_eq!(pairings.len(), 2);
        assert_ne!(pairings[0].white_version_id, pairings[1].white_version_id);
    }
}
