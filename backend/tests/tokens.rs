use cofounder_api::auth::tokens::{generate_token, hash_token};

#[test]
fn generated_tokens_are_url_safe_and_full_length() {
    let token = generate_token();

    // 32 bytes base64url-encoded without padding is 43 characters.
    assert_eq!(token.len(), 43);
    assert!(token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
}

#[test]
fn generated_tokens_are_unique() {
    let tokens: std::collections::HashSet<String> = (0..1000).map(|_| generate_token()).collect();

    assert_eq!(tokens.len(), 1000);
}

#[test]
fn hashing_is_deterministic() {
    assert_eq!(hash_token("abc"), hash_token("abc"));
}

#[test]
fn different_tokens_hash_differently() {
    assert_ne!(hash_token("abc"), hash_token("abd"));
}

#[test]
fn hash_is_32_bytes_and_not_the_token_itself() {
    let token = generate_token();
    let hash = hash_token(&token);

    assert_eq!(hash.len(), 32);
    assert_ne!(hash, token.as_bytes().to_vec());
}
