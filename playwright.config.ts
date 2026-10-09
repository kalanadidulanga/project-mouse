import { defineConfig } from "@playwright/test";

// One real app at a time: the tests share its window and the single-instance lock.
export default defineConfig({ testDir: "e2e", workers: 1, timeout: 60_000, reporter: "list" });
