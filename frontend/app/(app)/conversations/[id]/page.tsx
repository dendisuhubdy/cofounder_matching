import ThreadClient from "./thread-client";

export default async function ConversationPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = await params;

  return <ThreadClient conversationId={id} />;
}
