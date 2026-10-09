// The window (spec 006 FR-024): seven tabs down the left, one concern each, as in Move Mouse.
import { useState } from "react";
import Home from "./home";
import Movement from "./movement";
import Behaviour from "./behaviour";
import { Blackouts, Schedules } from "./timetable";
import Appearance from "./appearance";
import About from "./about";
import { Icon, type IconName } from "./icons";
import "./styles.css";

export type Page = "home" | "movement" | "behaviour" | "schedules" | "blackouts" | "appearance" | "about";

const PAGES: [Page, string, IconName][] = [
  ["home", "Home", "home"],
  ["movement", "Movement", "movement"],
  ["behaviour", "Behaviour", "behaviour"],
  ["schedules", "Schedules", "schedules"],
  ["blackouts", "Blackouts", "blackouts"],
  ["appearance", "Appearance", "appearance"],
  ["about", "About", "about"],
];

function PageView({ page, go }: { page: Page; go: (p: Page) => void }) {
  switch (page) {
    case "movement":
      return <Movement />;
    case "behaviour":
      return <Behaviour />;
    case "schedules":
      return <Schedules />;
    case "blackouts":
      return <Blackouts />;
    case "appearance":
      return <Appearance />;
    case "about":
      return <About />;
    default:
      return <Home go={go} />;
  }
}

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
        <PageView page={page} go={setPage} />
      </main>
    </div>
  );
}
