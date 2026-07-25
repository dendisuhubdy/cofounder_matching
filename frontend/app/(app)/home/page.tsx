import Link from "next/link";
import { cookies } from "next/headers";
import { MISSING_LABELS, ProfileView } from "@/lib/profile";

const BACKEND_URL = process.env.BACKEND_URL ?? "http://localhost:8080";

/**
 * A server component cannot use the /api rewrite, so it calls the backend
 * directly and forwards the incoming cookie header — the same approach
 * lib/session.ts takes for /me.
 */
async function getProfileView(): Promise<ProfileView | null> {
  const cookieHeader = (await cookies()).toString();

  const response = await fetch(`${BACKEND_URL}/me/profile`, {
    headers: { cookie: cookieHeader },
    cache: "no-store",
  });

  if (!response.ok) return null;
  return (await response.json()) as ProfileView;
}

export default async function HomePage() {
  const view = await getProfileView();

  if (!view) {
    return (
      <p role="alert" className="text-red-600">
        Could not load your profile. Reload to try again.
      </p>
    );
  }

  if (view.complete) {
    return (
      <div className="flex flex-col gap-2">
        <h1 className="text-2xl font-semibold">Your profile is complete</h1>
        <p className="text-neutral-600">
          You&apos;re in the deck, and other founders can see you.
        </p>
        <Link href="/deck" className="underline">
          Open your deck
        </Link>
      </div>
    );
  }

  return (
    <div className="flex max-w-xl flex-col gap-4">
      <div>
        <h1 className="text-2xl font-semibold">Finish your profile</h1>
        <p className="mt-1 text-neutral-600">
          You will not appear in anyone&apos;s deck until all of this is done.
        </p>
      </div>

      <ul className="flex flex-col gap-2">
        {view.missing.map((item) => (
          <li key={item} className="flex items-center gap-2">
            <span aria-hidden className="text-neutral-400">
              ○
            </span>
            <Link
              href={item === "responses" ? "/assessment" : "/profile"}
              className="underline"
            >
              {MISSING_LABELS[item] ?? item}
            </Link>
          </li>
        ))}
      </ul>
    </div>
  );
}
