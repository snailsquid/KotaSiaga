-- Identitas & Akses: app users and refresh tokens. Test data only (AGENTS.md section 2.8).
CREATE TABLE IF NOT EXISTS identity_users (
    id            TEXT PRIMARY KEY,
    email         TEXT NOT NULL UNIQUE,
    display_name  TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    is_test_account INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL
);

-- Only the SHA-256 of a refresh token is stored; the token itself is shown to the client once.
CREATE TABLE IF NOT EXISTS identity_refresh_tokens (
    token_hash  TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES identity_users(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    revoked_at  TEXT
);
CREATE INDEX IF NOT EXISTS identity_refresh_tokens_user ON identity_refresh_tokens(user_id);
