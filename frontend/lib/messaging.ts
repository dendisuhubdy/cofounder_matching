export interface ConversationSummary {
  id: string;
  other_user_id: string;
  other_display_name: string;
  other_headline: string;
  last_message: string | null;
  last_message_at: string | null;
  unread: number;
  matched: boolean;
}

export interface ConversationsView {
  conversations: ConversationSummary[];
}

export interface Message {
  id: string;
  conversation_id: string;
  sender_id: string;
  body: string;
  created_at: string;
  read_at: string | null;
}

export interface MessagesView {
  messages: Message[];
}

export interface OpenedConversation {
  id: string;
  created: boolean;
}

/** Mirrors `messaging::events::Event`, tagged by `type`. */
export type LiveEvent =
  | {
      type: "new_message";
      conversation_id: string;
      sender_id: string;
      preview: string;
    }
  | { type: "unread_count"; count: number };
