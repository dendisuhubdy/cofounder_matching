import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  use: { baseURL: "http://localhost:3000" },
  // Both servers are managed here so the suite is self-contained. The backend
  // must run with APP_ENV=test, which is what mounts /test/last-login-link —
  // without it the login-journey specs fail for environmental reasons that
  // look like product bugs.
  webServer: [
    {
      command: "cd ../backend && APP_ENV=test cargo run",
      url: "http://localhost:8080/test/last-login-link",
      reuseExistingServer: false,
      timeout: 180_000,
    },
    {
      command: "npm run dev",
      url: "http://localhost:3000",
      reuseExistingServer: true,
    },
  ],
});
