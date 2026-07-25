import { execFile } from "node:child_process";
import { promisify } from "node:util";

const run = promisify(execFile);

/**
 * Empties the e2e database before every run.
 *
 * The deck is drawn from every complete profile in the database and returns
 * only the top twenty by score. Run against a database that accumulates, the
 * accounts left behind by earlier runs crowd out the candidate a spec just
 * created, and the suite starts failing for reasons that have nothing to do
 * with the product.
 *
 * Truncating `users` cascades to profiles, interests, responses, trait
 * scores, swipes, matches and blocks — every table a spec touches.
 *
 * The database itself is created by the backend's webServer command, because
 * Playwright starts web servers before this runs. By the time this executes,
 * the backend has applied its migrations, so the tables exist.
 */
const E2E_DATABASE_URL =
  process.env.E2E_DATABASE_URL ??
  "postgres://postgres:postgres@localhost:5433/cofounder_e2e";

export default async function globalSetup(): Promise<void> {
  await run("psql", [E2E_DATABASE_URL, "-tAc", "TRUNCATE users CASCADE"]);
  console.log("[e2e] database cleared");
}
