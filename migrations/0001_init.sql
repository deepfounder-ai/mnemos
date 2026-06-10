-- mnemos initial schema
-- Multi-tenant per-user wiki with sources, FTS5 search, events, and lint view.
--
-- NOTE: PRAGMAs that cannot be changed inside a transaction (journal_mode,
-- synchronous, foreign_keys) are applied in `src/storage/pool.rs` *before*
-- the migration transaction is opened. Keeping them out of this file avoids
-- the SQLite error: "Safety level may not be changed inside a transaction".

-- ---------------------------------------------------------------------------
-- users
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
    id              TEXT PRIMARY KEY NOT NULL,
    username        TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);

-- ---------------------------------------------------------------------------
-- api_keys
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS api_keys (
    id              TEXT PRIMARY KEY NOT NULL,
    user_id         TEXT NOT NULL,
    name            TEXT NOT NULL,
    key_hash        TEXT NOT NULL UNIQUE,
    created_at      TEXT NOT NULL,
    last_used_at    TEXT,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_api_keys_key_hash ON api_keys(key_hash);

-- ---------------------------------------------------------------------------
-- pages
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS pages (
    id                  TEXT PRIMARY KEY NOT NULL,
    user_id             TEXT NOT NULL,
    slug                TEXT NOT NULL,
    title               TEXT NOT NULL,
    frontmatter_json    TEXT NOT NULL,
    body                TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    UNIQUE(user_id, slug),
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_pages_user_id ON pages(user_id);
CREATE INDEX IF NOT EXISTS idx_pages_user_slug ON pages(user_id, slug);
CREATE INDEX IF NOT EXISTS idx_pages_updated_at ON pages(user_id, updated_at);

-- ---------------------------------------------------------------------------
-- sources
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS sources (
    id              TEXT PRIMARY KEY NOT NULL,
    user_id         TEXT NOT NULL,
    source_id       TEXT NOT NULL,
    slug            TEXT NOT NULL,
    type            TEXT NOT NULL,           -- url | upload | session
    origin          TEXT,                    -- URL of origin, if any
    path            TEXT NOT NULL,           -- relative path inside user dir
    content_hash    TEXT NOT NULL,           -- sha256 of raw bytes
    created_at      TEXT NOT NULL,
    UNIQUE(user_id, source_id),
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_sources_user_id ON sources(user_id);
CREATE INDEX IF NOT EXISTS idx_sources_user_source_id ON sources(user_id, source_id);

-- ---------------------------------------------------------------------------
-- page_sources (many-to-many)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS page_sources (
    page_id      TEXT NOT NULL,
    source_id    TEXT NOT NULL,
    PRIMARY KEY(page_id, source_id),
    FOREIGN KEY(page_id) REFERENCES pages(id) ON DELETE CASCADE,
    FOREIGN KEY(source_id) REFERENCES sources(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_page_sources_source_id ON page_sources(source_id);

-- ---------------------------------------------------------------------------
-- page_relations (derived from related: frontmatter)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS page_relations (
    page_id        TEXT NOT NULL,
    related_slug   TEXT NOT NULL,
    user_id        TEXT NOT NULL,
    PRIMARY KEY(page_id, related_slug),
    FOREIGN KEY(page_id) REFERENCES pages(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_page_relations_user_id ON page_relations(user_id);
CREATE INDEX IF NOT EXISTS idx_page_relations_related_slug ON page_relations(user_id, related_slug);

-- ---------------------------------------------------------------------------
-- events (append-only log mirror)
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS events (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id         TEXT NOT NULL,
    kind            TEXT NOT NULL,           -- page.create | page.update | ...
    ref             TEXT,                    -- slug, source_id, key id, etc.
    ts              TEXT NOT NULL,
    payload_json    TEXT,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_events_user_ts ON events(user_id, ts);
CREATE INDEX IF NOT EXISTS idx_events_kind ON events(user_id, kind);

-- ---------------------------------------------------------------------------
-- FTS5 virtual table on pages
--
-- Default (non-contentless) FTS5 storage: FTS5 keeps its own copy of the
-- content in `pages_fts_content`, which is what `snippet()` and friends
-- read from. The triggers mirror writes on `pages` into the FTS5 index.
-- ---------------------------------------------------------------------------
CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(
    title,
    body,
    tags,
    tokenize='unicode61 remove_diacritics 2'
);

-- Sync FTS with pages table on insert/update/delete.
--
-- We use plain DELETE + INSERT instead of the FTS5 'delete' command, which
-- is more brittle (it requires the old column values to match exactly and
-- raises SQLITE_CONSTRAINT for duplicate rowids on rebuild). The
-- `WHERE rowid = old.rowid` clause is what keeps the index in sync.
CREATE TRIGGER IF NOT EXISTS pages_ai AFTER INSERT ON pages BEGIN
    INSERT INTO pages_fts(rowid, title, body, tags)
    VALUES (new.rowid, new.title, new.body, '');
END;

CREATE TRIGGER IF NOT EXISTS pages_ad AFTER DELETE ON pages BEGIN
    DELETE FROM pages_fts WHERE rowid = old.rowid;
END;

CREATE TRIGGER IF NOT EXISTS pages_au AFTER UPDATE ON pages BEGIN
    DELETE FROM pages_fts WHERE rowid = old.rowid;
    INSERT INTO pages_fts(rowid, title, body, tags)
    VALUES (new.rowid, new.title, new.body, '');
END;

-- ---------------------------------------------------------------------------
-- Convenience view: latest event per (user_id, kind)
-- ---------------------------------------------------------------------------
CREATE VIEW IF NOT EXISTS v_latest_events AS
    SELECT user_id, kind, ref, ts, payload_json
    FROM events e
    WHERE ts = (
        SELECT MAX(ts) FROM events e2
        WHERE e2.user_id = e.user_id AND e2.kind = e.kind
    );
