import VerifyClient from "./verify-client";

/**
 * Reading the token server-side keeps `useSearchParams` (and its Suspense
 * boundary requirement) out of the picture entirely; the client component
 * only needs to POST it.
 */
export default async function VerifyPage({
  searchParams,
}: {
  searchParams: Promise<{ [key: string]: string | string[] | undefined }>;
}) {
  const token = (await searchParams).token;
  return <VerifyClient token={typeof token === "string" ? token : null} />;
}
