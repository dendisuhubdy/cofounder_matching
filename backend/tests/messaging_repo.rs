use cofounder_api::messaging::repo;
use cofounder_api::users;
use sqlx::PgPool;
use uuid::Uuid;

async fn a_user(pool: &PgPool, email: &str, name: &str) -> Uuid {
    let id = users::repo::find_or_create_by_email(pool, email)
        .await
        .unwrap()
        .id;

    sqlx::query(
        "INSERT INTO profiles (user_id, display_name, headline, bio, roles, seeking_roles, commitment)
         VALUES ($1, $2, 'Building things', 'A bio.', ARRAY['engineering'], ARRAY['gtm'], 'full_time_now')
         ON CONFLICT (user_id) DO UPDATE SET display_name = EXCLUDED.display_name",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();

    id
}

#[sqlx::test]
async fn opening_a_conversation_creates_it_once(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;

    let (first, created) = repo::open(&pool, ada, grace).await.unwrap();
    assert!(created);

    // The other side opening it finds the same row, not a second one.
    let (second, created_again) = repo::open(&pool, grace, ada).await.unwrap();
    assert!(!created_again);
    assert_eq!(first.id, second.id);

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test]
async fn the_initiator_is_recorded(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;

    repo::open(&pool, ada, grace).await.unwrap();
    // Reopening from the other side must not rewrite who started it.
    repo::open(&pool, grace, ada).await.unwrap();

    let started_by: Uuid = sqlx::query_scalar("SELECT started_by FROM conversations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(started_by, ada);
}

#[sqlx::test]
async fn messages_come_back_oldest_first(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    repo::send(&pool, conversation.id, ada, "first").await.unwrap();
    repo::send(&pool, conversation.id, grace, "second").await.unwrap();
    repo::send(&pool, conversation.id, ada, "third").await.unwrap();

    let messages = repo::messages_in(&pool, conversation.id).await.unwrap();
    let bodies: Vec<&str> = messages.iter().map(|m| m.body.as_str()).collect();

    assert_eq!(bodies, vec!["first", "second", "third"]);
}

#[sqlx::test]
async fn sending_stamps_the_conversation(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    assert!(conversation.last_message_at.is_none());

    repo::send(&pool, conversation.id, ada, "hello").await.unwrap();

    let reloaded = repo::find_by_id(&pool, conversation.id)
        .await
        .unwrap()
        .unwrap();
    assert!(reloaded.last_message_at.is_some());
}

#[sqlx::test]
async fn a_conversation_lists_the_other_person_and_its_unread_count(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    repo::send(&pool, conversation.id, grace, "hello").await.unwrap();
    repo::send(&pool, conversation.id, grace, "are you there").await.unwrap();

    let for_ada = repo::for_user(&pool, ada).await.unwrap();
    assert_eq!(for_ada.len(), 1);
    assert_eq!(for_ada[0].other_display_name, "Grace");
    assert_eq!(for_ada[0].unread, 2);
    assert_eq!(for_ada[0].last_message.as_deref(), Some("are you there"));

    // Your own messages are never unread to you.
    let for_grace = repo::for_user(&pool, grace).await.unwrap();
    assert_eq!(for_grace[0].unread, 0);
}

#[sqlx::test]
async fn marking_read_clears_only_the_other_persons_messages(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    repo::send(&pool, conversation.id, grace, "hello").await.unwrap();
    repo::send(&pool, conversation.id, ada, "hi back").await.unwrap();

    let cleared = repo::mark_read(&pool, conversation.id, ada).await.unwrap();
    assert_eq!(cleared, 1);

    assert_eq!(repo::for_user(&pool, ada).await.unwrap()[0].unread, 0);

    // Ada's own message is still unread for Grace.
    assert_eq!(repo::for_user(&pool, grace).await.unwrap()[0].unread, 1);
}

#[sqlx::test]
async fn a_blocked_pair_disappears_from_both_conversation_lists(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();
    repo::send(&pool, conversation.id, ada, "hello").await.unwrap();

    sqlx::query("INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2)")
        .bind(grace)
        .bind(ada)
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo::for_user(&pool, ada).await.unwrap().is_empty());
    assert!(repo::for_user(&pool, grace).await.unwrap().is_empty());
}

#[sqlx::test]
async fn conversations_are_ordered_by_most_recent_activity(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let older = a_user(&pool, "older@example.com", "Older").await;
    let newer = a_user(&pool, "newer@example.com", "Newer").await;

    let (first, _) = repo::open(&pool, ada, older).await.unwrap();
    repo::send(&pool, first.id, ada, "hello").await.unwrap();

    let (second, _) = repo::open(&pool, ada, newer).await.unwrap();
    repo::send(&pool, second.id, ada, "hello").await.unwrap();

    let listed = repo::for_user(&pool, ada).await.unwrap();
    assert_eq!(listed[0].other_display_name, "Newer");
}

#[sqlx::test]
async fn started_conversations_are_counted_within_a_window(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let hopper = a_user(&pool, "hopper@example.com", "Hopper").await;

    repo::open(&pool, ada, grace).await.unwrap();
    repo::open(&pool, ada, hopper).await.unwrap();

    assert_eq!(repo::count_started_since(&pool, ada, 60).await.unwrap(), 2);
    // Being messaged does not consume the other person's allowance.
    assert_eq!(repo::count_started_since(&pool, grace, 60).await.unwrap(), 0);

    sqlx::query("UPDATE conversations SET created_at = now() - interval '2 hours'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(repo::count_started_since(&pool, ada, 60).await.unwrap(), 0);
}

#[sqlx::test]
async fn sent_messages_are_counted_within_a_window(pool: PgPool) {
    let ada = a_user(&pool, "ada@example.com", "Ada").await;
    let grace = a_user(&pool, "grace@example.com", "Grace").await;
    let (conversation, _) = repo::open(&pool, ada, grace).await.unwrap();

    repo::send(&pool, conversation.id, ada, "one").await.unwrap();
    repo::send(&pool, conversation.id, ada, "two").await.unwrap();
    repo::send(&pool, conversation.id, grace, "reply").await.unwrap();

    assert_eq!(repo::count_messages_since(&pool, ada, 1).await.unwrap(), 2);
    assert_eq!(repo::count_messages_since(&pool, grace, 1).await.unwrap(), 1);
}
