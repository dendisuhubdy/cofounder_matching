import { defineConfig } from "@playwright/test";

// A dedicated port, not Next's default 3000. Sharing 3000 means the suite
// silently adopts whatever dev server happens to be running -- including one
// belonging to an entirely different project -- and every spec then fails
// against the wrong app with a 404 that looks like a product bug.
const PORT = 3100;
const BASE_URL = `http://localhost:${PORT}`;

// A dedicated database for the same reason. The deck is drawn from every
// complete profile and returns only the top twenty, so accounts left behind
// by earlier runs crowd out the candidate a spec just created. global-setup
// creates this database and empties it before each run; the backend applies
// its migrations on boot.
const DATABASE_URL =
  process.env.E2E_DATABASE_URL ??
  "postgres://postgres:postgres@localhost:5433/cofounder_e2e";

const DATABASE_NAME = new URL(DATABASE_URL).pathname.replace(/^\//, "");
const ADMIN_URL = (() => {
  const url = new URL(DATABASE_URL);
  url.pathname = "/postgres";
  return url.toString();
})();

export default defineConfig({
  testDir: "./e2e",
  globalSetup: "./e2e/global-setup.ts",
  use: { baseURL: BASE_URL },
  // Both servers are managed here so the suite is self-contained. The backend
  // must run with APP_ENV=test, which is what mounts /test/last-login-link --
  // without it the login-journey specs fail for environmental reasons that
  // look like product bugs. BASE_URL must agree with the frontend's port, or
  // the emailed magic link points at a server the test never opened.
  webServer: [
    {
      // The database is created here rather than in globalSetup because
      // Playwright starts web servers first: by the time setup runs, the
      // backend has already tried to connect. The backend then applies its
      // own migrations on boot, so an empty database is enough.
      command:
        `psql "${ADMIN_URL}" -tAc "SELECT 1 FROM pg_database WHERE datname='${DATABASE_NAME}'" | grep -q 1 ` +
        `|| psql "${ADMIN_URL}" -c 'CREATE DATABASE "${DATABASE_NAME}"'; ` +
        `cd ../backend && APP_ENV=test BASE_URL=${BASE_URL} DATABASE_URL=${DATABASE_URL} cargo run`,
      // Still the test-only route, so a missing APP_ENV=test fails fast here
      // rather than as a puzzling spec failure. The address is a placeholder;
      // the route needs one and answers with a null link for an unknown user.
      url: "http://localhost:8080/test/last-login-link?email=healthcheck@example.com",
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
