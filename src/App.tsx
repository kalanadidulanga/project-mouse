// The window (spec 006 FR-024): tabs down the left, one concern each, as in Move Mouse.
import { useState } from "react";
import Home from "./home";
import Movement from "./movement";
import Settings from "./settings";
import Advanced from "./advanced";
import { Icon, type IconName } from "./icons";
import "./styles.css";

export type Page = "home" | "movement" | "behaviour" | "settings" | "advanced";

const PAGES: [Page, string, IconName][] = [
  ["home", "Home", "home"],
  ["movement", "Movement", "movement"],
  ["settings", "Settings", "behaviour"],
  ["advanced", "Advanced", "about"],
];

export default function App() {
  const [page, setPage] = useState<Page>("home");
  return (
    <div className="app">
      <nav className="rail" aria-label="Pages">
        <div className="brand">project-mouse</div>
        {PAGES.map(([id, label, icon]) => (
          <button
            key={id}
            className={page === id ? "active" : ""}
            aria-current={page === id ? "page" : undefined}
            onClick={() => setPage(id)}
          >
            <Icon name={icon} />
            {label}
          </button>
        ))}
      </nav>
      <main className="content">
        {page === "movement" ? (
          <Movement />
        ) : page === "settings" ? (
          <Settings />
        ) : page === "advanced" ? (
          <Advanced />
        ) : (
          <Home go={setPage} />
        )}
      </main>
    </div>
  );
}
