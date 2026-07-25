"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";
import { MatchSummary, MatchesView } from "@/lib/deck";

export default function MatchesClient() {
  const [matches, setMatches] = useState<MatchSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    apiFetch<MatchesView>("/matches")
      .then((view) => {
        setMatches(view.matches);
        setLoading(false);
      })
      .catch(() => {
        setError("Could not load your matches. Reload to try again.");
        setLoading(false);
      });
  }, []);

  if (loading) return <p className="text-neutral-600">Loading your matches…</p>;

  if (error) {
    return (
      <p role="alert" className="text-red-600">
        {error}
      </p>
    );
  }

  if (matches.length === 0) {
    return (
      <div className="flex max-w-xl flex-col gap-3">
        <h1 className="text-2xl font-semibold">No matches yet</h1>
        <p className="text-neutral-600">
          A match happens when you and another founder both swipe right.
        </p>
        <Link href="/deck" className="underline">
          Open your deck
        </Link>
      </div>
    );
  }

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <h1 className="text-2xl font-semibold">Your matches</h1>
      <ul className="flex flex-col gap-3">
        {matches.map((match) => (
          <li
            key={match.user_id}
            className="rounded-xl border border-neutral-200 p-4"
          >
            <p className="font-medium">{match.display_name}</p>
            {match.headline && (
              <p className="text-sm text-neutral-600">{match.headline}</p>
            )}
          </li>
        ))}
      </ul>
      <p className="text-sm text-neutral-600">
        Messaging arrives in the next slice.
      </p>
    </div>
  );
}
