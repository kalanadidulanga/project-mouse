// First run (spec FR-008 / SC-007): one question, three answers, each creating a working profile.
// None of them enables input synthesis — that stays off until the user asks for it explicitly.
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

const CHOICES: { id: string; title: string; body: string }[] = [
  {
    id: "long_job",
    title: "Finish a long job",
    body: "A build, a render, a transfer. The machine stays awake; the screen may still sleep. You pick which program it waits for.",
  },
  {
    id: "keep_screen",
    title: "Keep a screen up",
    body: "A dashboard, a presentation, something you are reading. The screen stays lit and unlocked until you turn it off.",
  },
  {
    id: "manual",
    title: "Set it up myself",
    body: "An empty profile. Drive the mode by hand, or build your own rules.",
  },
];

export default function FirstRun({ onDone }: { onDone: () => void }) {
  const [busy, setBusy] = useState(false);

  const pick = (choice: string) => {
    setBusy(true);
    invoke("complete_first_run", { choice })
      .then(onDone)
      .catch(() => setBusy(false));
  };

  return (
    <div className="firstrun">
      <h1>What do you need this for?</h1>
      <p className="note">
        This keeps your machine awake using the power API Windows provides for it. It does not
        change your power plan, and it releases everything when you quit. You can change any of
        this later.
      </p>

      <div className="choices">
        {CHOICES.map((c) => (
          <button
            key={c.id}
            className="choice"
            disabled={busy}
            onClick={() => pick(c.id)}
          >
            <strong>{c.title}</strong>
            <span>{c.body}</span>
          </button>
        ))}
      </div>

      <p className="note">
        Nothing here synthesizes input. Nothing types or moves your mouse. That is a separate
        switch in Settings, and it is off.
      </p>
    </div>
  );
}
