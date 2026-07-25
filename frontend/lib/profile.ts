export interface Choice {
  id: string;
  label: string;
}

export interface Options {
  roles: Choice[];
  idea_statuses: Choice[];
  stages: Choice[];
  commitments: Choice[];
  interests: Choice[];
}

export interface ProfileBody {
  display_name: string;
  headline: string;
  bio: string;
  city: string;
  country: string;
  timezone: string;
  linkedin_url: string | null;
  github_url: string | null;
  website_url: string | null;
  roles: string[];
  seeking_roles: string[];
  idea_status: string | null;
  stage: string | null;
  commitment: string | null;
  interests: string[];
}

export interface ProfileView {
  profile: ProfileBody;
  complete: boolean;
  missing: string[];
}

export const EMPTY_PROFILE: ProfileBody = {
  display_name: "",
  headline: "",
  bio: "",
  city: "",
  country: "",
  timezone: "",
  linkedin_url: "",
  github_url: "",
  website_url: "",
  roles: [],
  seeking_roles: [],
  idea_status: null,
  stage: null,
  commitment: null,
  interests: [],
};

// The API names what is missing; this turns those names into prose.
export const MISSING_LABELS: Record<string, string> = {
  bio: "Write a short bio",
  roles: "Pick what you bring",
  seeking_roles: "Pick what you're looking for",
  commitment: "Say how committed you are",
  responses: "Answer the 18 work-style questions",
};
