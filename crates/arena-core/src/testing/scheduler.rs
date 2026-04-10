use serde::{Deserialize, Serialize};

use super::EngineId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledPair {
    pub engine_a: EngineId,
    pub engine_b: EngineId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundRobinScheduler {
    pairings: Vec<ScheduledPair>,
    next_index: usize,
}

impl RoundRobinScheduler {
    pub fn new(participants: &[EngineId], repeats_per_pairing: u16) -> Self {
        let mut pairings = Vec::new();
        let repeats = usize::from(repeats_per_pairing.max(1));

        for (index, engine_a) in participants.iter().enumerate() {
            for engine_b in participants.iter().skip(index + 1) {
                for _ in 0..repeats {
                    pairings.push(ScheduledPair {
                        engine_a: *engine_a,
                        engine_b: *engine_b,
                    });
                }
            }
        }

        Self {
            pairings,
            next_index: 0,
        }
    }

    pub fn from_pairings(pairings: Vec<ScheduledPair>) -> Self {
        Self {
            pairings,
            next_index: 0,
        }
    }

    pub fn next_pair(&mut self) -> Option<ScheduledPair> {
        if self.pairings.is_empty() {
            return None;
        }

        let pair = self.pairings[self.next_index];
        self.next_index = (self.next_index + 1) % self.pairings.len();
        Some(pair)
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    #[test]
    fn scheduler_cycles_pairings() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let mut scheduler = RoundRobinScheduler::new(&[a, b, c], 1);

        let seen = vec![
            scheduler.next_pair().unwrap(),
            scheduler.next_pair().unwrap(),
            scheduler.next_pair().unwrap(),
            scheduler.next_pair().unwrap(),
        ];

        assert_eq!(seen[0], seen[3]);
        assert_eq!(seen.len(), 4);
    }
}
