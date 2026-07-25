"use client";

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";
import { DeckCard, DeckView, SwipeOutcome } from "@/lib/deck";
import { Choice, Options } from "@/lib/profile";

export default function DeckClient() {
  const [cards, setCards] = useState<DeckCard[]>([]);
  const [options, setOptions] = useState<Options | null>(null);
  const [complete, setComplete] = useState(true);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [matched, setMatched] = useState<DeckCard | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([apiFetch<DeckView>("/deck"), apiFetch<Options>("/options")])
      .then(([deck, loadedOptions]) => {
        setCards(deck.cards);
        setComplete(deck.profile_complete);
        setOptions(loadedOptions);
        setLoading(false);
      })
      .catch(() => {
        setError("Could not load your deck. Reload to try again.");
        setLoading(false);
      });
  }, []);

  const current = cards[0];

  const swipe = useCallback(
    async (direction: "left" | "right") => {
      if (!current || busy) return;

      setBusy(true);
      setError(null);

      try {
        const outcome = await apiFetch<SwipeOutcome>("/swipes", {
          method: "POST",
          body: JSON.stringify({
            target_id: current.user_id,
            direction,
          }),
        });

        if (outcome.matched) setMatched(current);
        // Permanent either way, so the card never returns to the deck.
        setCards((rest) => rest.slice(1));
      } catch {
        setError("That didn't save. Try again.");
      } finally {
        setBusy(false);
      }
    },
    [current, busy],
  );

  // Arrow keys are the keyboard equivalent of the two buttons. A modal is
  // open when a match happens, so keys are ignored until it is dismissed.
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (matched) return;
      if (event.key === "ArrowLeft") swipe("left");
      if (event.key === "ArrowRight") swipe("right");
    }

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [swipe, matched]);

  function labelFor(choices: Choice[] | undefined, id: string): string {
    return choices?.find((choice) => choice.id === id)?.label ?? id;
  }

  if (loading) return <p className="text-neutral-600">Loading your deck…</p>;

  if (!complete) {
    return (
      <div className="flex max-w-xl flex-col gap-3">
        <h1 className="text-2xl font-semibold">Finish your profile first</h1>
        <p className="text-neutral-600">
          The deck opens once your profile is complete — that is what keeps it
          free of empty accounts.
        </p>
        <Link href="/home" className="underline">
          See what&apos;s left
        </Link>
      </div>
    );
  }

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <h1 className="text-2xl font-semibold">Your deck</h1>

      {error && (
        <p role="alert" className="text-sm text-red-600">
          {error}
        </p>
      )}

      {matched && (
        <div
          role="dialog"
          aria-modal="true"
          aria-labelledby="match-heading"
          className="rounded-xl border border-neutral-900 bg-neutral-50 p-4"
        >
          <h2 id="match-heading" className="text-lg font-semibold">
            It&apos;s a match
          </h2>
          <p className="mt-1 text-neutral-700">
            You and {matched.display_name} both swiped right.
          </p>
          <div className="mt-3 flex gap-3">
            <Link href="/matches" className="underline">
              See your matches
            </Link>
            <button
              type="button"
              onClick={() => setMatched(null)}
              className="underline"
            >
              Keep swiping
            </button>
          </div>
        </div>
      )}

      {!current ? (
        <p id="deck-empty" className="text-neutral-600">
          That&apos;s everyone for now. Check back as more founders join.
        </p>
      ) : (
        <article className="flex flex-col gap-3 rounded-xl border border-neutral-200 p-5">
          <div className="flex items-baseline justify-between gap-3">
            <h2 className="text-xl font-semibold">{current.display_name}</h2>
            <span
              aria-label={`Match score ${current.score} out of 100`}
              className="text-sm text-neutral-600"
            >
              {current.score}
            </span>
          </div>

          {current.headline && (
            <p className="text-neutral-700">{current.headline}</p>
          )}

          {current.reasons.length > 0 && (
            <ul className="flex flex-wrap gap-2">
              {current.reasons.map((reason) => (
                <li
                  key={reason.component}
                  className="rounded-full bg-neutral-100 px-3 py-1 text-sm text-neutral-800"
                >
                  {reason.text}
                </li>
              ))}
            </ul>
          )}

          <p className="whitespace-pre-line text-neutral-700">{current.bio}</p>

          <dl className="flex flex-col gap-1 text-sm text-neutral-600">
            <div className="flex gap-2">
              <dt className="font-medium">Brings</dt>
              <dd>
                {current.roles
                  .map((role) => labelFor(options?.roles, role))
                  .join(", ")}
              </dd>
            </div>
            <div className="flex gap-2">
              <dt className="font-medium">Looking for</dt>
              <dd>
                {current.seeking_roles
                  .map((role) => labelFor(options?.roles, role))
                  .join(", ")}
              </dd>
            </div>
            {current.city && (
              <div className="flex gap-2">
                <dt className="font-medium">Based in</dt>
                <dd>
                  {current.city}
                  {current.country ? `, ${current.country}` : ""}
                </dd>
              </div>
            )}
          </dl>

          <div className="mt-2 flex gap-3">
            <button
              type="button"
              disabled={busy}
              onClick={() => swipe("left")}
              className="rounded-lg border border-neutral-300 px-4 py-2 disabled:opacity-50"
            >
              Pass
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => swipe("right")}
              className="rounded-lg bg-neutral-900 px-4 py-2 text-white disabled:opacity-50"
            >
              Interested
            </button>
          </div>
        </article>
      )}
    </div>
  );
}
