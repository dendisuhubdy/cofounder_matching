"use client";

import { useState } from "react";
import { ApiError, apiFetch } from "@/lib/api";

export default function LoginPage() {
  const [email, setEmail] = useState("");
  const [status, setStatus] = useState<"idle" | "sending" | "sent">("idle");
  const [error, setError] = useState<string | null>(null);

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    setStatus("sending");
    setError(null);

    try {
      await apiFetch("/auth/magic-link", {
        method: "POST",
        body: JSON.stringify({ email }),
      });
      setStatus("sent");
    } catch (err) {
      setStatus("idle");
      if (err instanceof ApiError) {
        setError(err.fieldError("email") ?? err.problem.title);
      } else {
        setError("Could not reach the server. Try again.");
      }
    }
  }

  if (status === "sent") {
    return (
      <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-3 p-6">
        <h1 className="text-2xl font-semibold">Check your email</h1>
        <p className="text-neutral-600">
          If {email} has an account, a sign-in link is on its way. It expires in
          15 minutes.
        </p>
      </main>
    );
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-6 p-6">
      <div>
        <h1 className="text-2xl font-semibold">Find a cofounder</h1>
        <p className="mt-1 text-neutral-600">
          Enter your email and we&apos;ll send you a sign-in link.
        </p>
      </div>

      <form onSubmit={onSubmit} className="flex flex-col gap-3">
        <label htmlFor="email" className="sr-only">
          Email
        </label>
        <input
          id="email"
          type="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@example.com"
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? "email-error" : undefined}
          className="rounded-lg border border-neutral-300 px-3 py-2"
        />

        {error && (
          <p id="email-error" role="alert" className="text-sm text-red-600">
            {error}
          </p>
        )}

        <button
          type="submit"
          disabled={status === "sending"}
          className="rounded-lg bg-neutral-900 px-3 py-2 text-white disabled:opacity-50"
        >
          {status === "sending" ? "Sending…" : "Send sign-in link"}
        </button>
      </form>
    </main>
  );
}
