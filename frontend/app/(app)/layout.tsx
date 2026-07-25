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
        <span className="font-semibold">Cofounder</span>
        <span className="text-sm text-neutral-600">{user.email}</span>
      </header>
      <main className="p-6">{children}</main>
    </div>
  );
}
