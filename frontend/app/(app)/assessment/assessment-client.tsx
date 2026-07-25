"use client";

import { useEffect, useState } from "react";
import { apiFetch } from "@/lib/api";

interface Question {
  id: string;
  text: string;
  axis: string;
}

interface ScalePoint {
  value: number;
  label: string;
}

interface Questionnaire {
  questions: Question[];
  scale: ScalePoint[];
}

interface Response {
  question_id: string;
  value: number;
}

interface ResponsesView {
  responses: Response[];
  answered: number;
  total: number;
  complete: boolean;
}

export default function AssessmentClient() {
  const [questionnaire, setQuestionnaire] = useState<Questionnaire | null>(
    null,
  );
  const [answers, setAnswers] = useState<Record<string, number>>({});
  const [view, setView] = useState<ResponsesView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      apiFetch<Questionnaire>("/questions"),
      apiFetch<ResponsesView>("/me/responses"),
    ])
      .then(([loaded, current]) => {
        setQuestionnaire(loaded);
        setAnswers(
          Object.fromEntries(
            current.responses.map((r) => [r.question_id, r.value]),
          ),
        );
        setView(current);
        setLoading(false);
      })
      .catch(() => {
        setError("Could not load the questions. Reload to try again.");
        setLoading(false);
      });
  }, []);

  async function answer(questionId: string, value: number) {
    // Optimistic: the radio should light up on click, not after a roundtrip.
    const previous = answers[questionId];
    setAnswers((current) => ({ ...current, [questionId]: value }));
    setError(null);

    try {
      const updated = await apiFetch<ResponsesView>("/me/responses", {
        method: "PUT",
        body: JSON.stringify({
          responses: [{ question_id: questionId, value }],
        }),
      });
      setView(updated);
    } catch {
      setAnswers((current) => {
        const rolledBack = { ...current };
        if (previous === undefined) {
          delete rolledBack[questionId];
        } else {
          rolledBack[questionId] = previous;
        }
        return rolledBack;
      });
      setError("That answer didn't save. Try again.");
    }
  }

  if (loading) {
    return <p className="text-neutral-600">Loading the questions…</p>;
  }

  if (!questionnaire) {
    return (
      <p role="alert" className="text-red-600">
        {error}
      </p>
    );
  }

  const answered = view?.answered ?? 0;
  const total = view?.total ?? questionnaire.questions.length;

  return (
    <div className="flex max-w-2xl flex-col gap-6">
      <div>
        <h1 className="text-2xl font-semibold">Work style</h1>
        <p className="mt-1 text-neutral-600">
          Eighteen statements. There are no right answers — answer as you
          actually work, not as you would like to. Each answer saves as you go.
        </p>
      </div>

      <p id="assessment-progress" role="status" className="text-sm font-medium">
        {answered === total
          ? "All 18 answered"
          : `${answered} of ${total} answered`}
      </p>

      {error && (
        <p role="alert" className="text-sm text-red-600">
          {error}
        </p>
      )}

      <ol className="flex flex-col gap-6">
        {questionnaire.questions.map((question, index) => (
          <li key={question.id} className="flex flex-col gap-2">
            <fieldset>
              <legend className="text-neutral-900">
                {index + 1}. {question.text}
              </legend>
              <div className="mt-2 flex flex-wrap gap-2">
                {questionnaire.scale.map((point) => {
                  const selected = answers[question.id] === point.value;
                  return (
                    <label
                      key={point.value}
                      className={`cursor-pointer rounded-lg border px-3 py-1 text-sm ${
                        selected
                          ? "border-neutral-900 bg-neutral-900 text-white"
                          : "border-neutral-300 text-neutral-700"
                      }`}
                    >
                      <input
                        type="radio"
                        name={question.id}
                        value={point.value}
                        checked={selected}
                        onChange={() => answer(question.id, point.value)}
                        className="sr-only"
                      />
                      {point.label}
                    </label>
                  );
                })}
              </div>
            </fieldset>
          </li>
        ))}
      </ol>
    </div>
  );
}
