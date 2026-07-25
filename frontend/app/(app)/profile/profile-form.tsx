"use client";

import { useEffect, useState } from "react";
import { ApiError, apiFetch } from "@/lib/api";
import {
  Choice,
  EMPTY_PROFILE,
  Options,
  ProfileBody,
  ProfileView,
} from "@/lib/profile";

type Status = "loading" | "ready" | "saving" | "saved" | "failed";

const inputClass = "w-full rounded-lg border border-neutral-300 px-3 py-2";

export default function ProfileForm() {
  const [options, setOptions] = useState<Options | null>(null);
  const [profile, setProfile] = useState<ProfileBody>(EMPTY_PROFILE);
  const [missing, setMissing] = useState<string[]>([]);
  const [status, setStatus] = useState<Status>("loading");
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      apiFetch<Options>("/options"),
      apiFetch<ProfileView>("/me/profile"),
    ])
      .then(([loadedOptions, view]) => {
        setOptions(loadedOptions);
        // The API returns null for unset links; the inputs need strings.
        setProfile({
          ...view.profile,
          linkedin_url: view.profile.linkedin_url ?? "",
          github_url: view.profile.github_url ?? "",
          website_url: view.profile.website_url ?? "",
        });
        setMissing(view.missing);
        setStatus("ready");
      })
      .catch(() => {
        setStatus("failed");
        setMessage("Could not load your profile. Reload to try again.");
      });
  }, []);

  function set<K extends keyof ProfileBody>(key: K, value: ProfileBody[K]) {
    setProfile((current) => ({ ...current, [key]: value }));
    setStatus("ready");
  }

  function toggle(key: "roles" | "seeking_roles" | "interests", id: string) {
    setProfile((current) => {
      const selected = current[key];
      return {
        ...current,
        [key]: selected.includes(id)
          ? selected.filter((value) => value !== id)
          : [...selected, id],
      };
    });
    setStatus("ready");
  }

  async function onSubmit(event: React.FormEvent) {
    event.preventDefault();
    setStatus("saving");
    setErrors({});
    setMessage(null);

    try {
      const view = await apiFetch<ProfileView>("/me/profile", {
        method: "PUT",
        body: JSON.stringify(profile),
      });
      setMissing(view.missing);
      setStatus("saved");
      setMessage("Profile saved");
    } catch (err) {
      setStatus("failed");
      if (err instanceof ApiError) {
        const fields: Record<string, string> = {};
        for (const problem of err.problem.errors ?? []) {
          fields[problem.field] = problem.message;
        }
        setErrors(fields);
        setMessage(
          err.problem.errors?.length
            ? "Some fields need attention"
            : err.problem.title,
        );
      } else {
        setMessage("Could not reach the server. Try again.");
      }
    }
  }

  if (status === "loading") {
    return <p className="text-neutral-600">Loading your profile…</p>;
  }

  if (!options) {
    return (
      <p role="alert" className="text-red-600">
        {message}
      </p>
    );
  }

  return (
    <form onSubmit={onSubmit} className="flex max-w-2xl flex-col gap-8">
      <div>
        <h1 className="text-2xl font-semibold">Your profile</h1>
        <p className="mt-1 text-neutral-600">
          {missing.length === 0
            ? "Your profile is complete."
            : `${missing.length} thing${missing.length === 1 ? "" : "s"} left before you appear in decks.`}
        </p>
      </div>

      <Section title="Identity">
        <Field label="Display name" error={errors.display_name}>
          <input
            id="display_name"
            value={profile.display_name}
            onChange={(e) => set("display_name", e.target.value)}
            className={inputClass}
          />
        </Field>
        <Field label="Headline" error={errors.headline}>
          <input
            id="headline"
            value={profile.headline}
            onChange={(e) => set("headline", e.target.value)}
            placeholder="One line on what you're building"
            className={inputClass}
          />
        </Field>
        <Field label="Bio" error={errors.bio}>
          <textarea
            id="bio"
            rows={5}
            value={profile.bio}
            onChange={(e) => set("bio", e.target.value)}
            className={inputClass}
          />
        </Field>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="City" error={errors.city}>
            <input
              id="city"
              value={profile.city}
              onChange={(e) => set("city", e.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label="Country" error={errors.country}>
            <input
              id="country"
              value={profile.country}
              onChange={(e) => set("country", e.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label="Timezone" error={errors.timezone}>
            <input
              id="timezone"
              value={profile.timezone}
              onChange={(e) => set("timezone", e.target.value)}
              placeholder="Europe/London"
              className={inputClass}
            />
          </Field>
        </div>
        <div className="grid gap-4 sm:grid-cols-3">
          <Field label="LinkedIn" error={errors.linkedin_url}>
            <input
              id="linkedin_url"
              value={profile.linkedin_url ?? ""}
              onChange={(e) => set("linkedin_url", e.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label="GitHub" error={errors.github_url}>
            <input
              id="github_url"
              value={profile.github_url ?? ""}
              onChange={(e) => set("github_url", e.target.value)}
              className={inputClass}
            />
          </Field>
          <Field label="Website" error={errors.website_url}>
            <input
              id="website_url"
              value={profile.website_url ?? ""}
              onChange={(e) => set("website_url", e.target.value)}
              className={inputClass}
            />
          </Field>
        </div>
      </Section>

      <Section title="What you bring">
        <ChoiceGroup
          legend="Your strengths"
          choices={options.roles}
          selected={profile.roles}
          onToggle={(id) => toggle("roles", id)}
          error={errors.roles}
        />
      </Section>

      <Section title="What you're looking for">
        <ChoiceGroup
          legend="Cofounder strengths"
          choices={options.roles}
          selected={profile.seeking_roles}
          onToggle={(id) => toggle("seeking_roles", id)}
          error={errors.seeking_roles}
        />
      </Section>

      <Section title="Where you are">
        <Field label="Idea status" error={errors.idea_status}>
          <select
            id="idea_status"
            value={profile.idea_status ?? ""}
            onChange={(e) => set("idea_status", e.target.value || null)}
            className={inputClass}
          >
            <option value="">Not set</option>
            {options.idea_statuses.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Stage" error={errors.stage}>
          <select
            id="stage"
            value={profile.stage ?? ""}
            onChange={(e) => set("stage", e.target.value || null)}
            className={inputClass}
          >
            <option value="">Not set</option>
            {options.stages.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Commitment" error={errors.commitment}>
          <select
            id="commitment"
            value={profile.commitment ?? ""}
            onChange={(e) => set("commitment", e.target.value || null)}
            className={inputClass}
          >
            <option value="">Not set</option>
            {options.commitments.map((choice) => (
              <option key={choice.id} value={choice.id}>
                {choice.label}
              </option>
            ))}
          </select>
        </Field>
      </Section>

      <Section title="Interests">
        <ChoiceGroup
          legend="Industries"
          choices={options.interests}
          selected={profile.interests}
          onToggle={(id) => toggle("interests", id)}
          error={errors.interests}
        />
      </Section>

      <div className="flex items-center gap-3">
        <button
          type="submit"
          disabled={status === "saving"}
          className="rounded-lg bg-neutral-900 px-4 py-2 text-white disabled:opacity-50"
        >
          {status === "saving" ? "Saving…" : "Save profile"}
        </button>
        {message && (
          <p
            id="profile-status"
            role="status"
            className={
              status === "failed"
                ? "text-sm text-red-600"
                : "text-sm text-green-700"
            }
          >
            {message}
          </p>
        )}
      </div>
    </form>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-lg font-medium">{title}</h2>
      {children}
    </section>
  );
}

function Field({
  label,
  error,
  children,
}: {
  label: string;
  error?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className="text-sm font-medium text-neutral-700">{label}</span>
      {children}
      {error && <p className="text-sm text-red-600">{error}</p>}
    </div>
  );
}

function ChoiceGroup({
  legend,
  choices,
  selected,
  onToggle,
  error,
}: {
  legend: string;
  choices: Choice[];
  selected: string[];
  onToggle: (id: string) => void;
  error?: string;
}) {
  return (
    <fieldset className="flex flex-col gap-2">
      <legend className="text-sm font-medium text-neutral-700">{legend}</legend>
      <div className="flex flex-wrap gap-2">
        {choices.map((choice) => {
          const active = selected.includes(choice.id);
          return (
            <button
              key={choice.id}
              type="button"
              aria-pressed={active}
              onClick={() => onToggle(choice.id)}
              className={`rounded-full border px-3 py-1 text-sm ${
                active
                  ? "border-neutral-900 bg-neutral-900 text-white"
                  : "border-neutral-300 text-neutral-700"
              }`}
            >
              {choice.label}
            </button>
          );
        })}
      </div>
      {error && <p className="text-sm text-red-600">{error}</p>}
    </fieldset>
  );
}
