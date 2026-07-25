import { expect, test } from "@playwright/test";
import { signIn } from "./helpers";

test("a founder fills in a profile and completes the assessment", async ({
  page,
  request,
}) => {
  await signIn(page, request, "ada");

  // A fresh account starts with nothing done.
  await expect(
    page.getByText("Answer the 18 work-style questions"),
  ).toBeVisible();

  await page.getByRole("link", { name: "Profile" }).click();
  await expect(page.getByRole("heading", { name: "Your profile" })).toBeVisible();

  await page.locator("#display_name").fill("Ada Lovelace");
  await page.locator("#headline").fill("Building tools for analytical engines");
  await page.locator("#bio").fill("Twenty years in numerical computing.");
  await page.locator("#city").fill("London");
  await page.locator("#country").fill("United Kingdom");
  await page.locator("#timezone").fill("Europe/London");
  await page.locator("#commitment").selectOption("full_time_now");

  await page
    .getByRole("group", { name: "Your strengths" })
    .getByRole("button", { name: "Engineering" })
    .click();
  await page
    .getByRole("group", { name: "Cofounder strengths" })
    .getByRole("button", { name: "GTM / Sales" })
    .click();
  await page
    .getByRole("group", { name: "Industries" })
    .getByRole("button", { name: "Developer tools" })
    .click();

  await page.getByRole("button", { name: "Save profile" }).click();
  await expect(page.locator("#profile-status")).toHaveText("Profile saved");

  // The profile is filled in but the assessment is not, so it is still incomplete.
  await expect(
    page.getByText("1 thing left before you appear in decks."),
  ).toBeVisible();

  await page.getByRole("link", { name: "Assessment" }).click();
  await expect(page.locator("#assessment-progress")).toHaveText(
    "0 of 18 answered",
  );

  const questions = page.getByRole("listitem");
  const count = await questions.count();
  expect(count).toBe(18);

  for (let index = 0; index < count; index++) {
    await questions.nth(index).getByText("Neutral").click();
    await expect(page.locator("#assessment-progress")).toHaveText(
      index + 1 === 18 ? "All 18 answered" : `${index + 1} of 18 answered`,
    );
  }

  await page.getByRole("link", { name: "Home" }).click();
  await expect(page.getByText("Your profile is complete")).toBeVisible();
});

test("an answer survives a reload", async ({ page, request }) => {
  await signIn(page, request, "grace");

  await page.goto("/assessment");
  const questions = page.getByRole("listitem");
  await questions.first().getByText("Strongly agree").click();
  await expect(page.locator("#assessment-progress")).toHaveText(
    "1 of 18 answered",
  );

  await page.reload();
  await expect(page.locator("#assessment-progress")).toHaveText(
    "1 of 18 answered",
  );
});

test("the profile form surfaces a server-side error inline", async ({
  page,
  request,
}) => {
  await signIn(page, request, "hopper");

  await page.goto("/profile");
  await page.locator("#website_url").fill("javascript:alert(1)");
  await page.getByRole("button", { name: "Save profile" }).click();

  await expect(
    page.getByText("must start with http:// or https://"),
  ).toBeVisible();
});
