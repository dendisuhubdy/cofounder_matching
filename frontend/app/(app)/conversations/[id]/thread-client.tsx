"use client";

import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, apiFetch } from "@/lib/api";
import {
  ConversationSummary,
  ConversationsView,
  LiveEvent,
  Message,
  MessagesView,
} from "@/lib/messaging";
import { User } from "@/lib/session";

// ReportDialog (@/components/report-dialog) is added by Task 9, which owns
// the moderation UI. Its usage here is intentionally omitted until then.

export default function ThreadClient({
  conversationId,
}: {
  conversationId: string;
}) {
  const [messages, setMessages] = useState<Message[]>([]);
  const [summary, setSummary] = useState<ConversationSummary | null>(null);
  const [me, setMe] = useState<User | null>(null);
  const [draft, setDraft] = useState("");
  const [loading, setLoading] = useState(true);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const load = useCallback(() => {
    return apiFetch<MessagesView>(`/conversations/${conversationId}/messages`)
      .then((view) => setMessages(view.messages))
      .catch(() => setError("Could not load this conversation."));
  }, [conversationId]);

  useEffect(() => {
    // The summary is fetched as well as the messages so the thread knows who
    // it is with before anyone has spoken. Deriving that from the messages
    // would leave a brand-new conversation with no counterpart, and the
    // moderation controls with nobody to act on.
    Promise.all([
      apiFetch<User>("/me"),
      apiFetch<ConversationsView>("/conversations"),
      load(),
    ])
      .then(([user, view]) => {
        setMe(user);
        setSummary(
          view.conversations.find((row) => row.id === conversationId) ?? null,
        );
        setLoading(false);
      })
      .catch(() => {
        // Without this, a `/me` or `/conversations` failure leaves `me` and
        // `summary` null while `load()` may still have populated messages —
        // rendering a thread with every message misattributed to the other
        // person, silently, since `mine` compares against a null `me.id`.
        setError("Could not load this conversation.");
        setLoading(false);
      });
  }, [conversationId, load]);

  // Only refetch for this conversation: an event about another thread would
  // otherwise mark its messages read behind the user's back, since fetching
  // a thread is what clears its unread count.
  useEffect(() => {
    const source = new EventSource("/api/events");

    source.onmessage = (event) => {
      const parsed = JSON.parse(event.data) as LiveEvent;
      if (
        parsed.type === "new_message" &&
        parsed.conversation_id === conversationId
      ) {
        load();
      }
    };

    return () => source.close();
  }, [conversationId, load]);

  useEffect(() => {
    bottom.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    if (!draft.trim() || sending) return;

    setSending(true);
    setError(null);

    try {
      const sent = await apiFetch<Message>(
        `/conversations/${conversationId}/messages`,
        { method: "POST", body: JSON.stringify({ body: draft }) },
      );
      setMessages((current) => [...current, sent]);
      setDraft("");
    } catch (err) {
      if (err instanceof ApiError) {
        setError(
          err.problem.type === "rate_limited"
            ? "You're sending messages too quickly. Wait a moment."
            : (err.fieldError("body") ?? err.problem.title),
        );
      } else {
        setError("Could not reach the server. Try again.");
      }
    } finally {
      setSending(false);
    }
  }

  if (loading) return <p className="text-neutral-600">Loading…</p>;

  // A failed initial load leaves `me` null forever (it's only ever set by
  // that load), which would otherwise make every message misattributed to
  // the other person (`mine` compares against a null id). Show the error
  // instead of a thread that looks like the other person said everything.
  if (error && !me) {
    return (
      <p role="alert" className="text-sm text-red-600">
        {error}
      </p>
    );
  }

  return (
    <div className="flex max-w-2xl flex-col gap-4">
      <Link href="/conversations" className="text-sm underline">
        ← All messages
      </Link>

      {summary && (
        <h1 className="text-xl font-semibold">{summary.other_display_name}</h1>
      )}

      <ol id="thread" className="flex flex-col gap-2">
        {messages.map((message) => {
          const mine = message.sender_id === me?.id;
          return (
            <li
              key={message.id}
              className={`max-w-[80%] rounded-xl px-3 py-2 ${
                mine
                  ? "self-end bg-neutral-900 text-white"
                  : "self-start bg-neutral-100 text-neutral-900"
              }`}
            >
              {message.body}
            </li>
          );
        })}
      </ol>
      <div ref={bottom} />

      {error && (
        <p role="alert" className="text-sm text-red-600">
          {error}
        </p>
      )}

      <form onSubmit={onSubmit} className="flex gap-2">
        <label htmlFor="message-body" className="sr-only">
          Message
        </label>
        <input
          id="message-body"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder="Write a message"
          disabled={sending}
          className="flex-1 rounded-lg border border-neutral-300 px-3 py-2"
        />
        <button
          type="submit"
          disabled={sending || !draft.trim()}
          className="rounded-lg bg-neutral-900 px-4 py-2 text-white disabled:opacity-50"
        >
          Send
        </button>
      </form>
    </div>
  );
}
