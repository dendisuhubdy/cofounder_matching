import { expect, test } from "@playwright/test";
import { uniqueEmail } from "./helpers";

const BACKEND = process.env.BACKEND_URL ?? "http://localhost:8080";

test("a new user signs in with a magic link", async ({ page, request }) => {
  const email = uniqueEmail("ada");

  await page.goto("/login");
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByRole("button", { name: "Send sign-in link" }).click();

  await expect(page.getByText("Check your email")).toBeVisible();

  const response = await request.get(
    `${BACKEND}/test/last-login-link?email=${encodeURIComponent(email)}`,
  );
  const { link } = await response.json();
  expect(link).toContain("/verify?token=");

  await page.goto(link);

  await expect(page).toHaveURL(/\/home$/);
  await expect(page.getByText(email)).toBeVisible();
});

test("a used link is rejected the second time", async ({ page, request }) => {
  const email = uniqueEmail("grace");

  await page.goto("/login");
  await page.getByPlaceholder("you@example.com").fill(email);
  await page.getByRole("button", { name: "Send sign-in link" }).click();
  await expect(page.getByText("Check your email")).toBeVisible();

  const { link } = await (
    await request.get(
      `${BACKEND}/test/last-login-link?email=${encodeURIComponent(email)}`,
    )
  ).json();

  await page.goto(link);
  await expect(page).toHaveURL(/\/home$/);

  await page.context().clearCookies();
  await page.goto(link);
  await expect(page.getByText("This link has expired")).toBeVisible();
});

test("an unauthenticated visitor is sent to login", async ({ page }) => {
  await page.goto("/home");
  await expect(page).toHaveURL(/\/login$/);
});

test("a malformed email is rejected inline", async ({ page }) => {
  await page.goto("/login");
  await page.waitForLoadState("networkidle");

  const input = page.getByPlaceholder("you@example.com");
  await input.fill("not-an-email");

  // Native type=email validation would block submission before any request is
  // sent. Disable form validation to reach the server-side check the API
  // actually enforces. noValidate is used rather than rewriting input.type
  // because React owns `type` from the JSX and restores it on re-render.
  await page.locator("form").evaluate((form) => {
    (form as HTMLFormElement).noValidate = true;
  });

  const rejected = page.waitForResponse(
    (r) => r.url().includes("/api/auth/magic-link") && r.status() === 422,
  );
  await page.getByRole("button", { name: "Send sign-in link" }).click();
  await rejected;

  // Addressed by id: Next renders its own role="alert" route announcer, so a
  // bare getByRole("alert") is ambiguous.
  await expect(page.locator("#email-error")).toHaveText(
    "must be a valid email address",
  );
});
