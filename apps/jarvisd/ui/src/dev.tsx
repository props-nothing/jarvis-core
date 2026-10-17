// Development playground for `npm run dev`. Not part of the embedded bundle (the build entry is catalog.tsx).
import "../../src/hud/hud.css";
import "./dev.css";
import { createRoot } from "react-dom/client";
import { useEffect, useState } from "react";
import { Catalog, type Integration } from "./catalog";

const samples: Integration[] = [
  { id: "connections", name: "GitHub", description: "Repositories, issues and pull requests. Sign in, check access or disconnect.", status: "Open to check account", tone: "info" },
  { id: "google", name: "Google", description: "Gmail and Calendar. Configure sign-in and allowed actions.", status: "Setup needed", tone: "todo" },
  { id: "voice", name: "Voice & web search", description: "ElevenLabs voice and web search credentials.", status: "Credentials saved", tone: "ok" },
  { id: "files", name: "Files & coding", description: "Folder access, coding agents and sandbox configuration.", status: "Configured", tone: "ok" },
  { id: "permissions", name: "Browser & documentation", description: "Built-in tools. Inspect available tools and choose when JARVIS asks.", status: "Built in · managed by JARVIS", tone: "info" },
  { id: "programs", name: "Custom tools (MCP)", description: "Connect a local program or remote tool server. For advanced integrations.", status: "Open to inspect servers", tone: "info" }
];
const accents = ["mint", "ice", "amethyst"] as const;
const atmospheres = ["cinematic", "quiet"] as const;

function Playground() {
  const [accent, setAccent] = useState<string>("mint");
  const [atmosphere, setAtmosphere] = useState<string>("cinematic");
  const [opened, setOpened] = useState("");
  useEffect(() => {
    document.documentElement.dataset.accent = accent;
    document.documentElement.dataset.atmosphere = atmosphere;
  }, [accent, atmosphere]);
  return <div className="sheet wide frame playground" role="dialog" aria-label="Controls playground">
    <div className="settings-heading"><h2>Playground</h2><span className="settings-context">{opened ? `navigate → ${opened}` : "Apps & tools"}</span></div>
    <div className="ops-tabs" role="group" aria-label="Accent">
      {accents.map(name => <button key={name} type="button" aria-pressed={accent === name} onClick={() => setAccent(name)}>{name}</button>)}
      {atmospheres.map(name => <button key={name} type="button" aria-pressed={atmosphere === name} onClick={() => setAtmosphere(name)}>{name}</button>)}
    </div>
    <Catalog entries={samples} navigate={setOpened} />
  </div>;
}

const host = document.getElementById("playground");
if (host) createRoot(host).render(<Playground />);
