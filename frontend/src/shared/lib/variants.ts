import type { AgentVersion, Variant } from "../api/types";

export function supportsVariant(version: AgentVersion, variant: Variant) {
  return version.capabilities?.supported_variants?.includes(variant) ?? true;
}
