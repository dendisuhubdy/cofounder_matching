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

  if (response.status === 204) return undefined as T;
  return (await response.json()) as T;
}
