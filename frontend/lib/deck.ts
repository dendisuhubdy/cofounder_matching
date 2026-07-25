export interface Reason {
  component: string;
  text: string;
}

export interface DeckCard {
  user_id: string;
  display_name: string;
  headline: string;
  bio: string;
  city: string;
  country: string;
  roles: string[];
  seeking_roles: string[];
  interests: string[];
  score: number;
  reasons: Reason[];
}

export interface DeckView {
  cards: DeckCard[];
  profile_complete: boolean;
}

export interface SwipeOutcome {
  matched: boolean;
}

export interface MatchSummary {
  user_id: string;
  display_name: string;
  headline: string;
  matched_at: string;
}

export interface MatchesView {
  matches: MatchSummary[];
}
