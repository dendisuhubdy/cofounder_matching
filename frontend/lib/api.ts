export interface ApiProblem {
  type: string;
  title: string;
  status: number;
  errors?: { field: string; message: string }[];
  retry_after?: number;
}

export class ApiError extends Error {
  constructor(public problem: ApiProblem) {
    super(problem.title);
    this.name = "ApiError";
  }

  fieldError(field: string): string | undefined {
    return this.problem.errors?.find((e) => e.field === field)?.message;
  }
}

export async function apiFetch<T>(
  path: string,
  init: RequestInit = {},
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    ...init,
    credentials: "include",
    headers: { "content-type": "application/json", ...(init.headers ?? {}) },
  });

  if (!response.ok) {
    const problem = (await response.json().catch(() => ({
      type: "internal_error",
      title: "something went wrong",
      status: response.status,
    }))) as ApiProblem;
    throw new ApiError(problem);
  }

  // Not every success carries a body: /auth/magic-link answers 202 with none
  // and /auth/logout answers 204. Parsing unconditionally throws on those and
  // makes a successful call look like a network failure.
  const text = await response.text();
  if (!text) return undefined as T;
  return JSON.parse(text) as T;
}
