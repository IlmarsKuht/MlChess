export function loadErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : "Request failed";
}
