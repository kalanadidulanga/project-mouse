// The window (spec 005, UI-UX §0.5): three pages, Home first. All state lives in Rust; each
// page reads what it shows.
import { useState } from "react";
import Home from "./home";
import Settings from "./settings";
import Advanced from "./advanced";
import "./styles.css";

type Page = "home" | "settings" | "advanced";

const PAGES: [Page, string][] = [
  ["home", "Home"],
  ["settings", "Settings"],
  ["advanced", "Advanced"],
];

export default function App() {
  const [page, setPage] = useState<Page>("home");
  return (
    <div className="app">
      <nav className="rail" aria-label="Pages">
        <div className="brand">project-mouse</div>
        {PAGES.map(([id, label]) => (
          <button
            key={id}
            className={page === id ? "active" : ""}
            aria-current={page === id ? "page" : undefined}
            onClick={() => setPage(id)}
          >
            {label}
          </button>
        ))}
      </nav>
      <main className="content">
        {page === "home" && <Home />}
        {page === "settings" && <Settings />}
        {page === "advanced" && <Advanced />}
      </main>
    </div>
  );
}
