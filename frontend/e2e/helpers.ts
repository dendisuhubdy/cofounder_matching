import { randomUUID } from "node:crypto";
import { APIRequestContext, Page, expect } from "@playwright/test";

const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";

/**
 * A brand-new address. Random rather than timestamped: spec files run in
 * parallel workers that start within the same millisecond, so `Date.now()`
 * collides. Two tests then share one account and race for its single-use
 * login link, and the loser fails with "this link has expired" against a link
 * it never used.
 */
export function uniqueEmail(prefix = "user"): string {
  return `${prefix}+${randomUUID()}@example.com`;
}

/**
 * A display name no other test or earlier run will have used.
 *
 * The deck is keyed by what a card shows, and locating a candidate by a
 * fixed name matches whoever happens to hold it — including someone left
 * behind by a previous run. A spec can then swipe on the wrong person and
 * fail somewhere far away from the cause.
 */
export function uniqueName(base: string): string {
  return `${base} ${randomUUID().slice(0, 8)}`;
}

/**
 * Signs a brand-new user in through the real magic-link flow and returns the
 * address used. The link is read from the test-only endpoint the backend
 * mounts when APP_ENV=test.
 */
export async function signIn(
  page: Page,
  request: APIRequestContext,
  prefix = "user",
): Promise<string> {
  const email = uniqueEmail(prefix);

  await page.goto("/login");
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByRole("button", { name: "Send sign-in link" }).click();
  await expect(page.getByText("Check your email")).toBeVisible();

  // Asked for by address: spec files run in parallel, so several sign-ins are
  // in flight at once and "the last link sent" would belong to another test.
  const { link } = await (
    await request.get(
      `${BACKEND}/test/last-login-link?email=${encodeURIComponent(email)}`,
    )
  ).json();

  await page.goto(link);
  await expect(page).toHaveURL(/\/home$/);

  return email;
}

/**
 * Brings the signed-in user to a complete profile through the API rather
 * than the UI. Driving eighteen questionnaire answers through the browser
 * for every deck test is slow and tests nothing the assessment specs do not
 * already cover. `page.request` shares the page's cookies, so this runs as
 * the signed-in user.
 */
export async function completeOnboarding(
  page: Page,
  overrides: Record<string, unknown> = {},
): Promise<void> {
  const profile = {
    display_name: "Test Founder",
    headline: "Building something",
    bio: "A real bio, long enough to count.",
    city: "Jakarta",
    country: "Indonesia",
    timezone: "Asia/Jakarta",
    roles: ["engineering"],
    seeking_roles: ["gtm"],
    idea_status: "committed_idea",
    stage: "prototype",
    commitment: "full_time_now",
    interests: ["ai_ml"],
    ...overrides,
  };

  const saved = await page.request.put("/api/me/profile", { data: profile });
  if (!saved.ok()) {
    throw new Error(
      `profile save failed: ${saved.status()} ${await saved.text()}`,
    );
  }

  const questions = await (await page.request.get("/api/questions")).json();
  const responses = questions.questions.map((question: { id: string }) => ({
    question_id: question.id,
    value: 3,
  }));

  const answered = await page.request.put("/api/me/responses", {
    data: { responses },
  });
  if (!answered.ok()) {
    throw new Error(
      `answers failed: ${answered.status()} ${await answered.text()}`,
    );
  }
}

/**
 * Pages through the deck until `name` is the card on top, passing on
 * everyone before them.
 *
 * The deck is drawn from every complete profile in the database, which in a
 * test run means candidates left behind by other specs and earlier runs. A
 * spec that assumes its own candidate is the first card is asserting
 * something the product never promised, and fails as soon as somebody else
 * scores higher.
 */
export async function swipeUntil(page: Page, name: string): Promise<void> {
  const heading = page.locator("article h2");
  const empty = page.locator("#deck-empty");

  // The deck holds at most twenty cards, so this cannot loop forever.
  for (let seen = 0; seen < 21; seen++) {
    await expect(heading.or(empty).first()).toBeVisible();

    if (await empty.isVisible()) {
      throw new Error(`${name} was never in the deck`);
    }

    const current = (await heading.textContent())?.trim() ?? "";
    if (current === name) return;

    await page.getByRole("button", { name: "Pass" }).click();

    // The next card renders asynchronously. Without waiting for the card to
    // actually change, the next pass reads the one already dismissed and
    // clicks again — skipping straight over the candidate being looked for.
    await page.waitForFunction((previous) => {
      const top = document.querySelector("article h2");
      return !top || top.textContent?.trim() !== previous;
    }, current);
  }

  throw new Error(`${name} was not reached within a full deck`);
}
