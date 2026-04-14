export function EngineDocumentation({ text }: { text: string }) {
  const blocks = text
    .trim()
    .split(/\n\s*\n/)
    .map((block) => block.trim())
    .filter(Boolean);

  return (
    <div className="engine-doc">
      {blocks.map((block, index) => {
        const lines = block
          .split("\n")
          .map((line) => line.trim())
          .filter(Boolean);
        if (lines.length === 0) {
          return null;
        }

        if (lines.length === 1 && lines[0].startsWith("## ")) {
          return (
            <h3 className="engine-doc-heading" key={index}>
              {lines[0].slice(3)}
            </h3>
          );
        }

        if (lines.length === 1 && lines[0].startsWith("### ")) {
          return (
            <h4 className="engine-doc-subheading" key={index}>
              {lines[0].slice(4)}
            </h4>
          );
        }

        if (lines.every((line) => line.startsWith("- "))) {
          return (
            <ul className="engine-doc-list" key={index}>
              {lines.map((line) => (
                <li key={line}>{line.slice(2)}</li>
              ))}
            </ul>
          );
        }

        return (
          <p className="engine-doc-paragraph" key={index}>
            {lines.join(" ")}
          </p>
        );
      })}
    </div>
  );
}
