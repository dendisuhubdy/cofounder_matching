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
