CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY CHECK(version > 0),
    name TEXT NOT NULL,
    applied_at INTEGER NOT NULL
) STRICT;
