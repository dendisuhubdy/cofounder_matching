use cofounder_api::email::console::RecordingMailer;
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
