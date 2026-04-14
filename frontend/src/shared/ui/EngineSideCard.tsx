export function EngineSideCard({
  side,
  title,
  name,
  clock,
  active = false
}: {
  side: "white" | "black";
  title: string;
  name: string;
  clock?: string;
  active?: boolean;
}) {
  return (
    <div className={`engine-card engine-card-${side} ${active ? "engine-card-active" : ""}`}>
      <span>{title}</span>
      <strong>{name}</strong>
      {clock ? <p>{clock}</p> : null}
    </div>
  );
}
