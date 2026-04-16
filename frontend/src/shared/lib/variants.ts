import type { AgentVersion, Variant } from "../../app/types";

export function supportsVariant(version: AgentVersion, variant: Variant) {
  return version.capabilities?.supported_variants?.includes(variant) ?? true;
}
