import Link from "next/link";
import { redirect } from "next/navigation";
import { getCurrentUser } from "@/lib/session";

export default async function AppLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  const user = await getCurrentUser();
  if (!user) redirect("/login");

  return (
    <div className="min-h-screen">
      <header className="flex items-center justify-between border-b border-neutral-200 px-6 py-3">
        <nav className="flex items-center gap-4">
          <span className="font-semibold">Cofounder</span>
          <Link href="/home" className="text-sm text-neutral-700 hover:underline">
            Home
          </Link>
          <Link
            href="/profile"
            className="text-sm text-neutral-700 hover:underline"
          >
            Profile
          </Link>
          <Link
            href="/assessment"
            className="text-sm text-neutral-700 hover:underline"
          >
            Assessment
          </Link>
          <Link href="/deck" className="text-sm text-neutral-700 hover:underline">
            Deck
          </Link>
          <Link
            href="/matches"
            className="text-sm text-neutral-700 hover:underline"
          >
            Matches
          </Link>
        </nav>
        <span className="text-sm text-neutral-600">{user.email}</span>
      </header>
      <main className="p-6">{children}</main>
    </div>
  );
}
