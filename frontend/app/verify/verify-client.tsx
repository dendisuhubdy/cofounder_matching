"use client";

import { useEffect, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { apiFetch } from "@/lib/api";

export default function VerifyClient({ token }: { token: string | null }) {
  const router = useRouter();
  const [failed, setFailed] = useState(false);
  const attempted = useRef(false);

  useEffect(() => {
    // A missing token needs no request; it is derived during render below
    // rather than set here, so the effect never calls setState synchronously.
    if (!token) return;

    // The token is single-use, so React's development double-render must not
    // consume it twice.
    if (attempted.current) return;
    attempted.current = true;

    apiFetch("/auth/verify", {
      method: "POST",
      body: JSON.stringify({ token }),
    })
      .then(() => {
        // refresh() discards the router cache so the authenticated layout
        // re-runs getCurrentUser() with the cookie that was just set.
        router.replace("/home");
        router.refresh();
      })
      .catch(() => setFailed(true));
  }, [token, router]);

  const showFailure = !token || failed;

  return (
    <main className="mx-auto flex min-h-screen max-w-md flex-col justify-center gap-3 p-6">
      {showFailure ? (
        <>
          <h1 className="text-2xl font-semibold">This link has expired</h1>
          <p className="text-neutral-600">
            Sign-in links last 15 minutes and work once.
          </p>
          <a href="/login" className="text-neutral-900 underline">
            Request a new one
          </a>
        </>
      ) : (
        <p className="text-neutral-600">Signing you in…</p>
      )}
    </main>
  );
}
