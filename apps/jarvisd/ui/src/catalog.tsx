import { createRoot } from "react-dom/client";

export interface Integration {
  id: string;
  name: string;
  description: string;
  status: string;
}

// Small product-specific symbols keep this catalog self-contained and offline.
const symbols: Record<string, string> = {
  connections: "M8 4v5a4 4 0 0 0 4 4h4M16 4v16M5 4h6M13 4h6M13 20h6",
  google: "M3 6h18v13H3zM3 6l9 7 9-7",
  voice: "M9 5a3 3 0 0 1 6 0v6a3 3 0 0 1-6 0zM5 10v1a7 7 0 0 0 14 0v-1M12 18v4M8 22h8",
  files: "M3 6h7l2 3h9v11H3z",
  permissions: "M3 4h18v16H3zM3 9h18M7 6.5h.01M10 6.5h.01M8 13l-2 2 2 2M16 13l2 2-2 2",
  programs: "M9 3v5M15 3v5M6 8h12v4a6 6 0 0 1-12 0zM12 18v4"
};

export function Catalog({ entries, navigate }: { entries: readonly Integration[]; navigate: (id: string) => void }) {
  return <section aria-label="Apps and tools catalog">
    <ul className="integration-list">
      {entries.map(entry => <li className="integration-item" key={entry.id}>
        <button className="integration-link" type="button" aria-label={`Open ${entry.name}`} aria-describedby={`integration-${entry.id}-status`} onClick={() => navigate(entry.id)}>
          <span className="integration-icon" aria-hidden="true"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"><path d={symbols[entry.id] || symbols.programs} /></svg></span>
          <span className="integration-copy"><span className="integration-name">{entry.name}</span><span className="integration-description">{entry.description}</span></span>
          <span className="integration-status" id={`integration-${entry.id}-status`}>{entry.status}</span>
          <svg className="integration-chevron" aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5"><path d="m9 5 7 7-7 7" /></svg>
        </button>
      </li>)}
    </ul>
    <p className="catalog-note">Setup status is shown here. Open a service to check its connection or change what JARVIS can use.</p>
    <p className="catalog-note">Connecting another assistant to JARVIS? <button className="link" type="button" onClick={() => navigate("connect")}>Open External access</button></p>
  </section>;
}

/** The legacy adapter owns the container and must call the returned disposer before replacing it. */
export function mountCatalog(parent: HTMLElement, entries: readonly Integration[], navigate: (id: string) => void): () => void {
  const container = document.createElement("div");
  parent.appendChild(container);
  const root = createRoot(container);
  root.render(<Catalog entries={entries} navigate={navigate} />);
  return () => { root.unmount(); container.remove(); };
}
