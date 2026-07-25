import { cookies } from "next/headers";

export interface User {
  id: string;
  email: string;
  status: string;
}

const BACKEND_URL = process.env.BACKEND_URL ?? "http://localhost:8080";

/**
 * Server components cannot use the /api rewrite, so they call the backend
 * directly and forward the incoming cookie header themselves.
 */
export async function getCurrentUser(): Promise<User | null> {
  const cookieHeader = (await cookies()).toString();
  if (!cookieHeader) return null;

  const response = await fetch(`${BACKEND_URL}/me`, {
    headers: { cookie: cookieHeader },
    cache: "no-store",
  });

  if (!response.ok) return null;
  return (await response.json()) as User;
}
