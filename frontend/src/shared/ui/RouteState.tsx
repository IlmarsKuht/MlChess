import { EmptyState } from "./EmptyState";

export function RouteLoadingState({ message = "Loading..." }: { message?: string }) {
  return (
    <section className="panel">
      <EmptyState>{message}</EmptyState>
    </section>
  );
}

export function RouteErrorState({ message }: { message: string }) {
  return (
    <section className="panel">
      <EmptyState>{message}</EmptyState>
    </section>
  );
}
