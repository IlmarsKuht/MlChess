export function EngineSideCard({
  side,
  title,
  name,
  clock,
  active = false,
  urgency = "normal",
  winner = false
}: {
  side: "white" | "black";
  title: string;
  name: string;
  clock?: string;
  active?: boolean;
  urgency?: "normal" | "warning" | "critical";
  winner?: boolean;
}) {
  return (
    <div
      className={`engine-card engine-card-${side} ${active ? "engine-card-active" : ""} ${
        urgency !== "normal" ? `engine-card-${urgency}` : ""
      } ${winner ? "engine-card-winner" : ""}`}
      data-urgency={urgency}
      data-winner={winner ? "true" : "false"}
    >
      <span>{title}</span>
      <strong>{name}</strong>
      {clock ? <p className="engine-card-clock">{clock}</p> : null}
    </div>
  );
}
