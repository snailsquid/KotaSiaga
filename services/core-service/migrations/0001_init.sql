-- core-service tables. Each bounded context owns the tables with its prefix and no other
-- context reads them (AGENTS.md section 2.4). Mock-generated reports stay in memory only.

-- Pelaporan Insiden: reports created by app users.
CREATE TABLE IF NOT EXISTS reports_reports (
    id            TEXT PRIMARY KEY,
    seq           INTEGER NOT NULL,          -- numeric part of the id, to continue numbering
    category      TEXT NOT NULL,
    status        TEXT NOT NULL,             -- waiting | verified | expired
    lat           REAL NOT NULL,
    lon           REAL NOT NULL,
    description   TEXT,
    severity      INTEGER NOT NULL,
    confirm_count INTEGER NOT NULL DEFAULT 0,
    dispute_count INTEGER NOT NULL DEFAULT 0,
    reporter      TEXT NOT NULL,             -- user id (usr_...)
    created_at    TEXT NOT NULL,
    verified_at   TEXT,
    expires_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS reports_reports_live ON reports_reports(status, expires_at);

-- Verifikasi & Reputasi: votes on user reports and app users' reputation.
CREATE TABLE IF NOT EXISTS verification_votes (
    report_id TEXT NOT NULL,
    user_id   TEXT NOT NULL,
    vote      TEXT NOT NULL,                 -- confirm | dispute
    weight    REAL NOT NULL,
    at        TEXT NOT NULL,
    PRIMARY KEY (report_id, user_id)
);
CREATE TABLE IF NOT EXISTS verification_reputation (
    user_id    TEXT PRIMARY KEY,
    reputation REAL NOT NULL
);

-- Peringatan & Umpan Kota: area subscriptions and the alerts raised for them.
CREATE TABLE IF NOT EXISTS alerts_subscriptions (
    id         TEXT PRIMARY KEY,
    user_id    TEXT NOT NULL,
    name       TEXT NOT NULL,
    lat        REAL NOT NULL,
    lon        REAL NOT NULL,
    radius_m   INTEGER NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS alerts_subscriptions_user ON alerts_subscriptions(user_id);

CREATE TABLE IF NOT EXISTS alerts_alerts (
    id              TEXT PRIMARY KEY,
    subscription_id TEXT NOT NULL REFERENCES alerts_subscriptions(id) ON DELETE CASCADE,
    user_id         TEXT NOT NULL,
    cell_id         TEXT NOT NULL,
    risk_level      TEXT NOT NULL,
    previous_level  TEXT NOT NULL,
    at              TEXT NOT NULL,
    message         TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS alerts_alerts_user_at ON alerts_alerts(user_id, at DESC);
