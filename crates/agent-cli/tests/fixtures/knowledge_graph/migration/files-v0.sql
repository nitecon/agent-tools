CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    path TEXT UNIQUE NOT NULL,
    extension TEXT NOT NULL DEFAULT '',
    size INTEGER NOT NULL DEFAULT 0,
    mtime_secs INTEGER NOT NULL,
    content_hash TEXT
);
INSERT INTO files VALUES (1, 'src/lib.rs', 'rs', 42, 1000, 'old-file-hash');
