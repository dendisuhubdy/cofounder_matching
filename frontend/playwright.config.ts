import { defineConfig } from "@playwright/test";

// A dedicated port, not Next's default 3000. Sharing 3000 means the suite
// silently adopts whatever dev server happens to be running -- including one
// belonging to an entirely different project -- and every spec then fails
// against the wrong app with a 404 that looks like a product bug.
const PORT = 3100;
const BASE_URL = `http://localhost:${PORT}`;

export default defineConfig({
  testDir: "./e2e",
  use: { baseURL: BASE_URL },
  // Both servers are managed here so the suite is self-contained. The backend
  // must run with APP_ENV=test, which is what mounts /test/last-login-link --
  // without it the login-journey specs fail for environmental reasons that
  // look like product bugs. BASE_URL must agree with the frontend's port, or
  // the emailed magic link points at a server the test never opened.
  webServer: [
    {
      command: `cd ../backend && APP_ENV=test BASE_URL=${BASE_URL} cargo run`,
      url: "http://localhost:8080/test/last-login-link",
      reuseExistingServer: false,
      timeout: 180_000,
    },
    {
      command: `npm run dev -- --port ${PORT}`,
      url: BASE_URL,
      reuseExistingServer: false,
      timeout: 120_000,
    },
  ],
});
