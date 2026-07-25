"use client";

import Link from "next/link";
import { useCallback, useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";
import { ConversationSummary, ConversationsView, LiveEvent } from "@/lib/messaging";

export default function ConversationsClient() {
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(() => {
    return apiFetch<ConversationsView>("/conversations")
      .then((view) => {
        setConversations(view.conversations);
        setLoading(false);
      })
      .catch(() => {
        setError("Could not load your conversations. Reload to try again.");
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // A new message anywhere changes this list, so the stream triggers a
  // refetch rather than trying to patch the list in place — the ordering and
  // unread counts are the server's to decide.
  useEffect(() => {
    const source = new EventSource("/api/events");

    source.onmessage = (event) => {
      const parsed = JSON.parse(event.data) as LiveEvent;
      if (parsed.type === "new_message") load();
    };

    return () => source.close();
  }, [load]);

  if (loading) {
    return <p className="text-neutral-600">Loading your conversations…</p>;
  }

  if (error) {
    return (
      <p role="alert" className="text-red-600">
        {error}
      </p>
    );
  }

  if (conversations.length === 0) {
    return (
      <div className="flex max-w-xl flex-col gap-3">
        <h1 className="text-2xl font-semibold">No conversations yet</h1>
        <p className="text-neutral-600">
          Message someone from your deck or your matches to start one.
        </p>
        <Link href="/deck" className="underline">
          Open your deck
        </Link>
      </div>
    );
  }

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <h1 className="text-2xl font-semibold">Messages</h1>
      <ul className="flex flex-col gap-2">
        {conversations.map((conversation) => (
          <li key={conversation.id}>
            <Link
              href={`/conversations/${conversation.id}`}
              className="flex items-start justify-between gap-3 rounded-xl border border-neutral-200 p-4 hover:border-neutral-400"
            >
              <span className="flex flex-col gap-1">
                <span className="font-medium">
                  {conversation.other_display_name}
                  {conversation.matched && (
                    <span className="ml-2 rounded-full bg-neutral-100 px-2 py-0.5 text-xs text-neutral-700">
                      Match
                    </span>
                  )}
                </span>
                <span className="text-sm text-neutral-600">
                  {conversation.last_message ?? "No messages yet"}
                </span>
              </span>
              {conversation.unread > 0 && (
                <span
                  aria-label={`${conversation.unread} unread`}
                  className="rounded-full bg-neutral-900 px-2 py-0.5 text-xs text-white"
                >
                  {conversation.unread}
                </span>
              )}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
