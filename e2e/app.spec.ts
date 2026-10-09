// End-to-end: launches the real app (Rust + WebView2) from a fresh folder and drives its window over
// the WebView2 DevTools port. Run `npm run test:e2e`; it builds first, so the binary is never stale.
// Close any running project-mouse first: single-instance would hand the launch to that copy.
import { test, expect, chromium, type Browser, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { copyFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const PORT = 9333;
let app: ChildProcess;
let browser: Browser;
let page: Page;
let dir: string;
const errors: string[] = [];

test.describe.configure({ mode: "serial" });

async function connect(): Promise<Browser> {
  for (let i = 0; i < 60; i++) {
    if (app.exitCode !== null) throw new Error("project-mouse exited at launch. Is another copy running?");
    try {
      return await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
    } catch {
      await new Promise((r) => setTimeout(r, 500));
    }
  }
  throw new Error("No DevTools port after 30 s");
}

test.beforeAll(async () => {
  // The config lives beside the exe, so a copy in a fresh folder is a fresh install.
  dir = mkdtempSync(join(tmpdir(), "pm-e2e-"));
  const exe = join(dir, "project-mouse.exe");
  copyFileSync("src-tauri/target/debug/project-mouse.exe", exe);
  app = spawn(exe, [], {
    stdio: "ignore",
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
  });
  browser = await connect();
  const ctx = browser.contexts()[0];
  page = ctx.pages()[0] ?? (await ctx.waitForEvent("page"));
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  page.on("response", (r) => r.url().includes("ipc.localhost") && !r.ok() && errors.push(`${r.url()} ${r.status()}`));
  await page.getByRole("navigation", { name: "Pages" }).waitFor(); // first load done
  await page.reload(); // so errors from a first load are caught too
});

test.afterAll(async () => {
  await browser?.close();
  app?.kill();
  await new Promise((r) => setTimeout(r, 1000)); // let Windows release the exe
  rmSync(dir, { recursive: true, force: true, maxRetries: 5 });
});

// Saves are optimistic in the UI, so wait for the backend's answer before reloading.
const saved = () => page.waitForResponse((r) => r.url().includes("ipc.localhost/set_"));
const tab = (name: string) => page.getByRole("navigation", { name: "Pages" }).getByRole("button", { name });

test("every tab loads its settings", async () => {
  await expect(page.getByRole("status").first()).toHaveText(/Stopped/);
  const loaded: [string, RegExp][] = [
    ["Movement", /What happens/],
    ["Behaviour", /When running/],
    ["Schedules", /No schedules yet/],
    ["Blackouts", /No blackouts yet/],
    ["Appearance", /Keep this window above other windows/],
    ["About", /System idle for/],
  ];
  for (const [name, text] of loaded) {
    await tab(name).click();
    await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
    await expect(page.getByText(text).first()).toBeVisible();
  }
  expect(errors).toEqual([]);
});

test("Start and Stop", async () => {
  await tab("Home").click();
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByRole("status").first()).toHaveText(/Running/);
  await page.getByRole("button", { name: "Stop" }).click();
  await expect(page.getByRole("status").first()).toHaveText(/Stopped/);
  expect(errors).toEqual([]);
});

test("settings survive a reload", async () => {
  await tab("Movement").click();
  await Promise.all([saved(), page.getByLabel("Direction", { exact: true }).selectOption("Circle")]);
  await tab("Schedules").click();
  await Promise.all([saved(), page.getByRole("button", { name: "+ Stop time" }).click()]);
  await expect(page.getByLabel("Action for schedule 1")).toHaveValue("Stop");

  await page.reload();
  await tab("Schedules").click();
  await expect(page.getByLabel("Action for schedule 1")).toHaveValue("Stop");
  await tab("Movement").click();
  await expect(page.getByLabel("Direction", { exact: true })).toHaveValue("Circle");
  expect(errors).toEqual([]);
});
