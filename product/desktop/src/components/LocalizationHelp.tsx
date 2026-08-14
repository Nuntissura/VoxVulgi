import { useEffect, useState } from "react";
import { safeLocalStorageGet, safeLocalStorageSet } from "../lib/persist";

export type LocalizationHelpContent = {
  what: string;
  when: string;
  steps: string[];
  concepts?: Record<string, string>;
};

const HELP_ALL_KEY = "voxvulgi.v1.loc.help_all";
const HELP_ALL_EVENT = "voxvulgi:localization-help-all";

function readShowAllHelp(): boolean {
  return safeLocalStorageGet(HELP_ALL_KEY) === "1";
}

function publishShowAllHelp(value: boolean): void {
  safeLocalStorageSet(HELP_ALL_KEY, value ? "1" : "0");
  window.dispatchEvent(new CustomEvent<boolean>(HELP_ALL_EVENT, { detail: value }));
}

function useShowAllHelp(): [boolean, (value: boolean) => void] {
  const [showAll, setShowAll] = useState(readShowAllHelp);

  useEffect(() => {
    const onLocalChange = (event: Event) => {
      setShowAll(Boolean((event as CustomEvent<boolean>).detail));
    };
    const onStorage = (event: StorageEvent) => {
      if (event.key === HELP_ALL_KEY) setShowAll(event.newValue === "1");
    };
    window.addEventListener(HELP_ALL_EVENT, onLocalChange);
    window.addEventListener("storage", onStorage);
    return () => {
      window.removeEventListener(HELP_ALL_EVENT, onLocalChange);
      window.removeEventListener("storage", onStorage);
    };
  }, []);

  return [showAll, publishShowAllHelp];
}

export function LocalizationHelpAllToggle() {
  const [showAll, setShowAll] = useShowAllHelp();
  return (
    <label
      style={{ display: "inline-flex", alignItems: "center", gap: 6, fontSize: 12 }}
      title="Open or close every Localization Studio help panel. This choice is remembered."
    >
      <input
        type="checkbox"
        checked={showAll}
        onChange={(event) => setShowAll(event.currentTarget.checked)}
        data-testid="localization-help-all"
        data-agent-safe-action="true"
      />
      Show all help
    </label>
  );
}

export function LocalizationHelpButton({
  helpId,
  content,
}: {
  helpId: string;
  content: LocalizationHelpContent;
}) {
  const [localOpen, setLocalOpen] = useState(false);
  const [showAll] = useShowAllHelp();
  const open = showAll || localOpen;
  const helpName = helpId
    .replace(/^loc-home-/, "")
    .replace(/^loc-/, "")
    .replace(/-/g, " ");

  return (
    <>
      <button
        type="button"
        onClick={() => setLocalOpen((value) => !value)}
        disabled={showAll}
        title={
          showAll
            ? "This help is open because Show all help is enabled"
            : open
              ? "Hide help"
              : "Show help"
        }
        aria-label={`${open ? "Hide" : "Show"} help for ${helpName}`}
        aria-expanded={open}
        aria-controls={`localization-help-panel-${helpId}`}
        data-testid={`localization-help-${helpId}`}
        data-agent-safe-action="true"
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          width: 22,
          height: 22,
          borderRadius: "50%",
          border: "1px solid rgba(100,120,140,0.4)",
          background: open ? "rgba(59,81,105,0.15)" : "transparent",
          color: "#4b5563",
          fontSize: 13,
          fontWeight: 700,
          cursor: showAll ? "default" : "pointer",
          marginLeft: 8,
          verticalAlign: "middle",
          flexShrink: 0,
        }}
      >
        ?
      </button>
      {open ? (
        <div
          id={`localization-help-panel-${helpId}`}
          data-testid={`localization-help-panel-${helpId}`}
          style={{
            width: "100%",
            marginTop: 8,
            padding: "10px 14px",
            borderRadius: 8,
            background: "rgba(59,81,105,0.08)",
            border: "1px solid rgba(100,120,140,0.2)",
            fontSize: 13,
            lineHeight: 1.5,
          }}
        >
          <div style={{ marginBottom: 6 }}>
            <strong>What this does:</strong> {content.what}
          </div>
          <div style={{ marginBottom: 6 }}>
            <strong>When to use it:</strong> {content.when}
          </div>
          {content.steps.length > 0 ? (
            <div style={{ marginBottom: content.concepts ? 6 : 0 }}>
              <strong>Typical workflow:</strong>
              <ol style={{ margin: "4px 0 0 0", paddingLeft: 20 }}>
                {content.steps.map((step, index) => (
                  <li key={`${helpId}-step-${index}`}>{step}</li>
                ))}
              </ol>
            </div>
          ) : null}
          {content.concepts ? (
            <div>
              <strong>Key concepts:</strong>
              <ul style={{ margin: "4px 0 0 0", paddingLeft: 20 }}>
                {Object.entries(content.concepts).map(([term, definition]) => (
                  <li key={term}>
                    <strong>{term}</strong> — {definition}
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
        </div>
      ) : null}
    </>
  );
}
