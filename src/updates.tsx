import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { SettingRow, Switch } from "./controls";

type UpdateStatus = { current: string; available: string | null; auto_check: boolean };

/** UPDATES.md §6: a background check hints, it never interrupts. But the hint used to be a tray
 *  tooltip and nothing else, which is not a hint so much as a secret. This is the affordance
 *  ROADMAP M5 asks for — visible, and still nothing happens until you press it. */
export function UpdateBanner() {
  const [u, setU] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const read = () => invoke<UpdateStatus>("get_update_status").then(setU).catch(() => {});
    read();
    const t = window.setInterval(read, 5000);
    return () => window.clearInterval(t);
  }, []);

  if (!u?.available) return null;
  return (
    <div className="update-banner">
      <span>
        Version <strong>{u.available}</strong> is available. You are on {u.current}.
      </span>
      <button
        className="btn primary"
        disabled={busy}
        onClick={() => {
          setBusy(true);
          invoke("install_update").catch(() => setBusy(false));
        }}
      >
        {busy ? "Installing…" : "Install and restart"}
      </button>
    </div>
  );
}

/** Settings → Updates. The switch is required by UPDATES.md §6: some people run this on
 *  locked-down machines where an outbound request gets noticed, and they deserve the choice. */
export function UpdateSettings() {
  const [u, setU] = useState<UpdateStatus | null>(null);
  const [checking, setChecking] = useState(false);
  const [checked, setChecked] = useState(false);

  const read = useCallback(
    () => invoke<UpdateStatus>("get_update_status").then(setU).catch(() => {}),
    [],
  );
  useEffect(() => {
    read();
  }, [read]);

  if (!u) return null;

  const checkNow = () => {
    setChecking(true);
    setChecked(false);
    invoke("check_for_update").finally(() => {
      // The check runs in the background; give it a moment, then re-read.
      window.setTimeout(() => {
        read().then(() => {
          setChecking(false);
          setChecked(true);
        });
      }, 2500);
    });
  };

  return (
    <section className="section">
      <h2>Updates</h2>
      <SettingRow title="This version">
        <span>{u.current}</span>
      </SettingRow>
      <SettingRow title="Check for updates automatically" note="A check only tells you an update exists. It never installs one on its own.">
        <Switch
          label="Check for updates automatically"
          on={u.auto_check}
          onChange={(v) => invoke("set_auto_update", { enabled: v }).then(read)}
        />
      </SettingRow>
      <div className="cond-row" style={{ marginTop: 8 }}>
        <button className="btn" onClick={checkNow} disabled={checking}>
          {checking ? "Checking…" : "Check now"}
        </button>
        <span className="note">
          {u.available
            ? `Version ${u.available} is available. Install it from Home.`
            : checked
              ? "You are up to date."
              : "Checks run about every six hours."}
        </span>
      </div>
    </section>
  );
}
