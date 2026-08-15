CREATE TABLE files (
    id INTEGER PRIMARY KEY,
    path TEXT UNIQUE NOT NULL,
    mtime_secs INTEGER NOT NULL,
    mtime_nanos INTEGER NOT NULL
);
CREATE TABLE symbols (
    id INTEGER PRIMARY KEY,
    file_id INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    language TEXT NOT NULL,
    parent TEXT,
    UNIQUE(file_id, name, start_line)
);
INSERT INTO files VALUES (1, 'src/lib.rs', 1000, 0);
INSERT INTO symbols VALUES (1, 1, 'dispatch', 'function', 1, 3, 'rust', NULL);
