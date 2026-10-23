-- Tucklane schema v1. Paths are stored relative to the authorized root
-- using '/' separators. No file contents are ever stored.

CREATE TABLE settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,          -- JSON-encoded typed value
    updated_at  TEXT NOT NULL
);

CREATE TABLE authorized_roots (
    id              TEXT PRIMARY KEY,
    canonical_path  TEXT NOT NULL,
    display_path    TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('active', 'revoked')),
    selected_at     TEXT NOT NULL,
    revoked_at      TEXT
);
-- v1 supports exactly one active root.
CREATE UNIQUE INDEX idx_roots_single_active ON authorized_roots(status) WHERE status = 'active';

CREATE TABLE rules (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    description         TEXT NOT NULL DEFAULT '',
    priority            INTEGER NOT NULL,
    enabled             INTEGER NOT NULL DEFAULT 1,
    match_mode          TEXT NOT NULL DEFAULT 'all',
    conditions          TEXT NOT NULL,      -- JSON array
    action              TEXT NOT NULL,      -- JSON object
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    last_triggered_at   TEXT,
    match_count         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_rules_priority ON rules(priority);

CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,
    root_id     TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('manual', 'auto', 'undo', 'retry')),
    status      TEXT NOT NULL CHECK (status IN ('running', 'completed', 'cancelled', 'failed', 'interrupted')),
    started_at  TEXT NOT NULL,
    ended_at    TEXT,
    total       INTEGER NOT NULL DEFAULT 0,
    completed   INTEGER NOT NULL DEFAULT 0,
    skipped     INTEGER NOT NULL DEFAULT 0,
    failed      INTEGER NOT NULL DEFAULT 0,
    undone_at   TEXT
);
CREATE INDEX idx_sessions_started ON sessions(started_at);

CREATE TABLE operations (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    root_id         TEXT NOT NULL,
    kind            TEXT NOT NULL CHECK (kind IN ('move', 'undo', 'recycle')),
    state           TEXT NOT NULL CHECK (state IN ('planned', 'in_progress', 'completed', 'skipped', 'failed', 'undone', 'recovery_required')),
    source_rel      TEXT NOT NULL,
    dest_dir_rel    TEXT NOT NULL,
    dest_rel        TEXT,                   -- final path once completed
    original_name   TEXT NOT NULL,
    final_name      TEXT,
    size            INTEGER,
    mtime_ms        INTEGER,
    file_id         TEXT,                   -- volume serial + file index
    sha256          TEXT,                   -- only when copy-verify was used
    method          TEXT,
    rule_id         TEXT,
    rule_name       TEXT,
    reason          TEXT,
    error           TEXT,
    undo_of         TEXT REFERENCES operations(id),
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    completed_at    TEXT
);
CREATE INDEX idx_ops_session ON operations(session_id);
CREATE INDEX idx_ops_state ON operations(state);
CREATE INDEX idx_ops_created ON operations(created_at);
CREATE INDEX idx_ops_source ON operations(source_rel);
CREATE INDEX idx_ops_dest ON operations(dest_rel);

CREATE TABLE file_index (
    root_id         TEXT NOT NULL,
    rel_path        TEXT NOT NULL,
    size            INTEGER NOT NULL,
    mtime_ms        INTEGER NOT NULL,
    category        TEXT,
    status          TEXT NOT NULL,           -- ready | pending | review | ignored | locked
    last_scanned_at TEXT NOT NULL,
    PRIMARY KEY (root_id, rel_path)
);

CREATE TABLE duplicate_groups (
    id          TEXT PRIMARY KEY,
    root_id     TEXT NOT NULL,
    hash        TEXT NOT NULL,
    size        INTEGER NOT NULL,
    members     TEXT NOT NULL,              -- JSON array of {rel, mtimeMs}
    scanned_at  TEXT NOT NULL
);
CREATE INDEX idx_dups_root ON duplicate_groups(root_id);

CREATE TABLE app_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    severity    TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error')),
    kind        TEXT NOT NULL,
    message     TEXT NOT NULL,
    session_id  TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_events_created ON app_events(created_at);

CREATE TABLE exclusions (
    root_id     TEXT NOT NULL,
    rel_path    TEXT NOT NULL,
    reason      TEXT NOT NULL,              -- user | undo
    created_at  TEXT NOT NULL,
    PRIMARY KEY (root_id, rel_path)
);

CREATE TABLE saved_plans (
    id          TEXT PRIMARY KEY,
    root_id     TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    item_count  INTEGER NOT NULL,
    items       TEXT NOT NULL               -- JSON array of {sourceRel, destDir, selected}
);
