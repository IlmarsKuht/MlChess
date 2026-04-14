import type { Participant } from "../../app/types";

export function participantName(participant?: Participant | null, fallback = "Player") {
  return participant?.display_name ?? fallback;
}
