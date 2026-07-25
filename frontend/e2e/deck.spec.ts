import { expect, test } from "@playwright/test";
import { completeOnboarding, signIn, swipeUntil, uniqueName } from "./helpers";

/// The counterpart to `completeOnboarding`'s defaults: brings GTM and wants
/// engineering, so the pair scores the whole role budget against each other.
const COMPLEMENTARY = {
  roles: ["gtm"],
  seeking_roles: ["engineering"],
};

test("a founder sees a scored candidate and can pass on them", async ({
  page,
  browser,
  request,
}) => {
  // A candidate for the deck to contain.
  const grace = uniqueName("Grace Hopper");

  const other = await browser.newContext();
  const otherPage = await other.newPage();
  await signIn(otherPage, otherPage.request, "candidate");
  await completeOnboarding(otherPage, {
    display_name: grace,
    ...COMPLEMENTARY,
  });
  await otherPage.close();
  await other.close();

  await signIn(page, request, "viewer");
  await completeOnboarding(page);

  await page.getByRole("link", { name: "Deck" }).click();
  await expect(page.getByRole("heading", { name: "Your deck" })).toBeVisible();

  await swipeUntil(page, grace);

  // The card explains itself.
  await expect(page.getByText("They bring GTM / Sales")).toBeVisible();

  await page.getByRole("button", { name: "Pass" }).click();

  // A pass is permanent, so a reload does not bring them back.
  await page.reload();
  await expect(page.getByRole("heading", { name: grace })).toBeHidden();
});

test("an incomplete profile is told to finish before swiping", async ({
  page,
  request,
}) => {
  await signIn(page, request, "empty");

  await page.goto("/deck");

  await expect(
    page.getByRole("heading", { name: "Finish your profile first" }),
  ).toBeVisible();
});

test("a mutual right swipe creates a match both founders can see", async ({
  page,
  browser,
  request,
}) => {
  const ada = uniqueName("Ada Lovelace");
  const grace = uniqueName("Grace Hopper");

  await signIn(page, request, "first");
  await completeOnboarding(page, { display_name: ada });

  const secondContext = await browser.newContext();
  const secondPage = await secondContext.newPage();
  await signIn(secondPage, secondPage.request, "second");
  await completeOnboarding(secondPage, {
    display_name: grace,
    ...COMPLEMENTARY,
  });

  // Ada swipes right on Grace first; no match yet.
  await page.goto("/deck");
  await swipeUntil(page, grace);
  await page.getByRole("button", { name: "Interested" }).click();
  await expect(page.getByRole("dialog")).toBeHidden();

  // Grace swipes back, which completes the match.
  await secondPage.goto("/deck");
  await swipeUntil(secondPage, ada);
  await secondPage.getByRole("button", { name: "Interested" }).click();

  await expect(secondPage.getByRole("dialog")).toBeVisible();
  await expect(secondPage.getByText("It's a match")).toBeVisible();

  // Both sides see it on the matches page.
  await secondPage.getByRole("link", { name: "See your matches" }).click();
  await expect(secondPage.getByText(ada)).toBeVisible();

  await page.goto("/matches");
  await expect(page.getByText(grace)).toBeVisible();

  await secondPage.close();
  await secondContext.close();
});

test("the deck can be driven from the keyboard", async ({
  page,
  browser,
  request,
}) => {
  const alan = uniqueName("Alan Turing");

  const otherContext = await browser.newContext();
  const otherPage = await otherContext.newPage();
  await signIn(otherPage, otherPage.request, "keyboardtarget");
  await completeOnboarding(otherPage, {
    display_name: alan,
    ...COMPLEMENTARY,
  });
  await otherPage.close();
  await otherContext.close();

  await signIn(page, request, "keyboard");
  await completeOnboarding(page);

  await page.goto("/deck");
  await swipeUntil(page, alan);

  await page.keyboard.press("ArrowLeft");

  await expect(page.getByRole("heading", { name: alan })).toBeHidden();
});
