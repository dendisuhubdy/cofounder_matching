use cofounder_api::email::console::{LastLinkMailer, RecordingMailer};
use cofounder_api::email::Mailer;

#[tokio::test]
async fn recording_mailer_captures_recipient_and_link() {
    let mailer = RecordingMailer::default();

    mailer
        .send_login_link("ada@example.com", "http://localhost:3000/verify?token=xyz")
        .await
        .unwrap();

    let sent = mailer.sent();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0, "ada@example.com");
    assert_eq!(sent[0].1, "http://localhost:3000/verify?token=xyz");
}

#[tokio::test]
async fn recording_mailer_accumulates_across_sends() {
    let mailer = RecordingMailer::default();

    mailer
        .send_login_link("a@example.com", "link-a")
        .await
        .unwrap();
    mailer
        .send_login_link("b@example.com", "link-b")
        .await
        .unwrap();

    assert_eq!(mailer.sent().len(), 2);
}

#[tokio::test]
async fn last_link_mailer_keeps_one_link_per_recipient() {
    // The e2e suite runs specs in parallel, so two sign-ins are routinely in
    // flight at once. A single shared "last link" slot lets one browser
    // consume the other's token, which surfaces as a 400 on a valid-looking
    // link.
    let mailer = LastLinkMailer::default();

    mailer
        .send_login_link("ada@example.com", "link-for-ada")
        .await
        .unwrap();
    mailer
        .send_login_link("grace@example.com", "link-for-grace")
        .await
        .unwrap();

    assert_eq!(
        mailer.last_for("ada@example.com"),
        Some("link-for-ada".to_string())
    );
    assert_eq!(
        mailer.last_for("grace@example.com"),
        Some("link-for-grace".to_string())
    );
}

#[tokio::test]
async fn last_link_mailer_returns_the_most_recent_link_for_an_address() {
    let mailer = LastLinkMailer::default();

    mailer.send_login_link("ada@example.com", "old").await.unwrap();
    mailer.send_login_link("ada@example.com", "new").await.unwrap();

    assert_eq!(mailer.last_for("ada@example.com"), Some("new".to_string()));
}

#[tokio::test]
async fn last_link_mailer_matches_addresses_case_insensitively() {
    // Addresses are normalized before the link is issued; the lookup has to
    // agree or the test endpoint reports no link for a user who has one.
    let mailer = LastLinkMailer::default();

    mailer
        .send_login_link("ada@example.com", "link-for-ada")
        .await
        .unwrap();

    assert_eq!(
        mailer.last_for("  Ada@Example.COM  "),
        Some("link-for-ada".to_string())
    );
}

#[tokio::test]
async fn last_link_mailer_has_nothing_for_an_unknown_address() {
    let mailer = LastLinkMailer::default();

    assert_eq!(mailer.last_for("nobody@example.com"), None);
}
