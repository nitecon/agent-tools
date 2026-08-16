use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, ErrorCode, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Component, Path};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const CURRENT_SCHEMA_VERSION: i64 = 2;
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

pub struct ProjectIndex {
    conn: Connection,
    ephemeral: bool,
}

#[derive(Debug, Clone)]
pub struct ResourceInput<'a> {
    pub project_id: &'a str,
    pub namespace: &'a str,
    pub external_id: &'a str,
    pub canonical_uri: &'a str,
    pub kind: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub origin_kind: &'a str,
    pub origin_id: &'a str,
    pub authority: &'a str,
    pub status: Option<&'a str>,
    pub stale_after: Option<&'a str>,
    pub metadata: &'a Value,
}

#[derive(Debug, Clone)]
pub struct ResourceVersionInput<'a> {
    pub resource_id: i64,
    pub revision: &'a str,
    pub source_format: &'a str,
    pub media_type: Option<&'a str>,
    pub body: Option<&'a str>,
    pub raw_metadata: Option<&'a str>,
    pub content_hash: &'a str,
    pub generated_by: Option<&'a str>,
    pub generated_at: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct ContentSegmentInput<'a> {
    pub ordinal: usize,
    pub title: &'a str,
    pub heading_path: Option<&'a str>,
    pub text: &'a str,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
    pub token_count: Option<usize>,
    pub content_hash: &'a str,
    pub metadata: &'a Value,
}

#[derive(Debug, Clone)]
pub struct EdgeInput<'a> {
    pub src_resource_id: i64,
    pub dst_resource_id: Option<i64>,
    pub dst_ref: Option<&'a str>,
    pub relation: &'a str,
    pub confidence: &'a str,
    pub extractor: &'a str,
    pub source_resource_id: i64,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
    pub content_hash: &'a str,
    pub metadata: &'a Value,
}

#[derive(Debug, Clone)]
pub struct ProvenanceInput<'a> {
    pub resource_version_id: i64,
    pub source_resource_id: Option<i64>,
    pub source_ref: &'a str,
    pub author: Option<&'a str>,
    pub usage_count: Option<u64>,
    pub last_modified: Option<&'a str>,
    pub metadata: &'a Value,
}

#[derive(Debug, Clone)]
pub struct VerificationInput<'a> {
    pub resource_version_id: i64,
    pub actor_resource_id: i64,
    pub verified_at: Option<&'a str>,
    pub verification_kind: &'a str,
    pub metadata: &'a Value,
}

#[derive(Debug, Clone)]
pub struct FileMetadataInput<'a> {
    pub project_id: &'a str,
    pub project_root: &'a Path,
    pub path: &'a Path,
    pub language: Option<&'a str>,
    pub extension: &'a str,
    pub size: u64,
    pub mtime_secs: i64,
    pub mtime_nanos: i64,
    pub content_hash: Option<&'a str>,
    pub scan_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub resource_id: i64,
    pub mtime_secs: i64,
    pub mtime_nanos: i64,
    pub size: u64,
    pub content_hash: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SymbolSnapshotInput<'a> {
    pub stable_key: &'a str,
    pub name: &'a str,
    pub kind: &'a str,
    pub parent_stable_key: Option<&'a str>,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: Option<usize>,
    pub end_byte: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct RelationshipSnapshotInput<'a> {
    pub source_stable_key: Option<&'a str>,
    pub dst_ref: &'a str,
    pub relation: &'a str,
    pub confidence: &'a str,
    pub start_line: usize,
    pub end_line: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub content_hash: &'a str,
    pub metadata: &'a Value,
}

#[derive(Debug, Clone)]
pub struct CodeSnapshotInput<'a> {
    pub file: FileMetadataInput<'a>,
    pub source: &'a str,
    pub extractor: &'a str,
    pub symbols: &'a [SymbolSnapshotInput<'a>],
    pub relationships: &'a [RelationshipSnapshotInput<'a>],
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceMatch {
    pub id: i64,
    pub canonical_uri: String,
    pub namespace: String,
    pub kind: String,
    pub title: String,
    pub authority: String,
    pub origin_kind: String,
    pub origin_id: String,
    pub status: String,
    pub current_version_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchMatch {
    pub resource: ResourceMatch,
    pub segment_id: i64,
    pub heading_path: Option<String>,
    pub text: String,
    pub rank_micros: i64,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilter<'a> {
    pub namespace: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub status: Option<&'a str>,
    pub origin: Option<&'a str>,
    pub path: Option<&'a str>,
    pub language: Option<&'a str>,
    pub relation: Option<&'a str>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceDetail {
    pub resource: ResourceMatch,
    pub description: Option<String>,
    pub stale_after: Option<String>,
    pub metadata: Value,
    pub revision: Option<String>,
    pub source_format: Option<String>,
    pub content_hash: Option<String>,
    pub generated_by: Option<String>,
    pub generated_at: Option<String>,
    pub tags: Vec<String>,
    pub provenance_count: usize,
    pub verification_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeMatch {
    pub id: i64,
    pub src_resource_id: i64,
    pub dst_resource_id: Option<i64>,
    pub dst_ref: Option<String>,
    pub relation: String,
    pub confidence: String,
    pub extractor: String,
    pub source_version_id: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyMigrationStats {
    pub files_seen: usize,
    pub files_migrated: usize,
    pub symbols_seen: usize,
    pub symbols_migrated: usize,
    pub versions_migrated: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolutionStats {
    pub edges_seen: usize,
    pub resolved: usize,
    pub unresolved: usize,
    pub ambiguous: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TraversedEdge {
    pub id: i64,
    pub depth: usize,
    pub direction: String,
    pub relation: String,
    pub confidence: String,
    pub source_uri: String,
    pub source_title: String,
    pub target_uri: Option<String>,
    pub target_title: Option<String>,
    pub unresolved_ref: Option<String>,
    pub source_path: Option<String>,
    pub start_line: Option<usize>,
}

impl ProjectIndex {
    pub fn open(db_path: &Path) -> Result<Self> {
        Self::open_with_busy_timeout(db_path, SQLITE_BUSY_TIMEOUT)
    }

    fn open_with_busy_timeout(db_path: &Path, busy_timeout: Duration) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("failed to open project index at {}", db_path.display()))?;
        conn.busy_timeout(busy_timeout)
            .context("failed to configure project index lock timeout")?;
        Self::configure(&conn)?;
        Self::migrate_schema(&conn)?;
        Ok(Self {
            conn,
            ephemeral: false,
        })
    }

    pub fn open_for_project(project_root: &Path) -> Result<Self> {
        let db_path = agent_core::project_data_dir(project_root).join("project.db");
        Self::open_persistent_or_ephemeral(&db_path, SQLITE_BUSY_TIMEOUT)
    }

    pub fn open_persistent_or_ephemeral(db_path: &Path, busy_timeout: Duration) -> Result<Self> {
        match Self::open_with_busy_timeout(db_path, busy_timeout) {
            Ok(index) => Ok(index),
            Err(err) if is_sqlite_lock_error(&err) => Err(err.context(format!(
                "project index at {} is busy or locked after waiting {}ms",
                db_path.display(),
                busy_timeout.as_millis()
            ))),
            Err(_) => Self::open_ephemeral(),
        }
    }

    pub fn open_ephemeral() -> Result<Self> {
        let conn =
            Connection::open_in_memory().context("failed to open in-memory project index")?;
        Self::configure(&conn)?;
        Self::migrate_schema(&conn)?;
        Ok(Self {
            conn,
            ephemeral: true,
        })
    }

    fn configure(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(())
    }

    fn migrate_schema(conn: &Connection) -> Result<()> {
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > CURRENT_SCHEMA_VERSION {
            bail!(
                "project index schema version {version} is newer than supported version {CURRENT_SCHEMA_VERSION}"
            );
        }
        if version == CURRENT_SCHEMA_VERSION {
            return Ok(());
        }

        conn.execute_batch("BEGIN IMMEDIATE")?;
        let migration = (|| -> Result<()> {
            // Steps are additive and applied in order, so an index at any
            // earlier version upgrades in place without a rebuild.
            if version < 1 {
                conn.execute_batch(SCHEMA_V1)?;
                conn.execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    params![1, now_epoch_seconds()],
                )?;
            }
            if version < 2 {
                conn.execute_batch(SCHEMA_V2)?;
                conn.execute(
                    "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                    params![2, now_epoch_seconds()],
                )?;
            }
            conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION)?;
            Ok(())
        })();
        match migration {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(err) => {
                let _ = conn.execute_batch("ROLLBACK");
                return Err(err);
            }
        }
        Ok(())
    }

    pub fn is_ephemeral(&self) -> bool {
        self.ephemeral
    }

    /// Read-only compatibility access for query adapters during index migration.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    pub fn file_state(&self, path: &str) -> Result<Option<FileState>> {
        self.conn
            .query_row(
                "SELECT resource_id, mtime_secs, mtime_nanos, size, content_hash
                 FROM files WHERE path = ?1",
                params![path],
                |row| {
                    Ok(FileState {
                        resource_id: row.get(0)?,
                        mtime_secs: row.get(1)?,
                        mtime_nanos: row.get(2)?,
                        size: u64::try_from(row.get::<_, i64>(3)?).unwrap_or(0),
                        content_hash: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn mark_file_seen(&self, path: &str, scan_id: &str) -> Result<()> {
        if path.is_empty() || scan_id.is_empty() {
            bail!("path and scan_id must be non-empty");
        }
        let updated = self.conn.execute(
            "UPDATE files SET scan_id = ?1 WHERE path = ?2",
            params![scan_id, path],
        )?;
        if updated == 0 {
            bail!("cannot mark unknown file as seen: {path}");
        }
        Ok(())
    }

    pub fn producer_is_current(
        &self,
        path: &str,
        producer: &str,
        input_hash: &str,
    ) -> Result<bool> {
        if path.is_empty() || producer.is_empty() || input_hash.is_empty() {
            bail!("path, producer, and input_hash must be non-empty");
        }
        let current = self
            .conn
            .query_row(
                "SELECT ps.input_hash
                 FROM producer_state ps JOIN files f ON f.resource_id = ps.source_resource_id
                 WHERE f.path = ?1 AND ps.producer = ?2",
                params![path, producer],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(current.as_deref() == Some(input_hash))
    }

    pub fn upsert_file_metadata(&self, input: &FileMetadataInput<'_>) -> Result<i64> {
        let path = normalized_repo_path(input.project_root, input.path)?;
        let uri = canonical_repo_uri(input.project_id, input.project_root, input.path)?;
        let metadata = serde_json::json!({});
        let resource_id = self.ensure_resource(&ResourceInput {
            project_id: input.project_id,
            namespace: "file",
            external_id: &path,
            canonical_uri: &uri,
            kind: "file",
            title: &path,
            description: None,
            origin_kind: "repository",
            origin_id: input.project_id,
            authority: "repository",
            status: None,
            stale_after: None,
            metadata: &metadata,
        })?;
        self.conn.execute(
            "INSERT INTO files (
                resource_id, path, language, extension, size, mtime_secs, mtime_nanos,
                content_hash, scan_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(resource_id) DO UPDATE SET
                path = excluded.path,
                language = COALESCE(excluded.language, files.language),
                extension = excluded.extension,
                size = excluded.size,
                mtime_secs = excluded.mtime_secs,
                mtime_nanos = excluded.mtime_nanos,
                content_hash = COALESCE(excluded.content_hash, files.content_hash),
                scan_id = COALESCE(excluded.scan_id, files.scan_id)",
            params![
                resource_id,
                path,
                input.language,
                input.extension,
                i64::try_from(input.size)?,
                input.mtime_secs,
                input.mtime_nanos,
                input.content_hash,
                input.scan_id,
            ],
        )?;
        if let Some(hash) = input.content_hash {
            self.put_version(&ResourceVersionInput {
                resource_id,
                revision: hash,
                source_format: "source",
                media_type: None,
                body: None,
                raw_metadata: None,
                content_hash: hash,
                generated_by: None,
                generated_at: None,
            })?;
        }
        Ok(resource_id)
    }

    pub fn complete_file_scan(&self, project_id: &str, scan_id: &str) -> Result<usize> {
        if project_id.is_empty() || scan_id.is_empty() {
            bail!("project_id and scan_id must be non-empty");
        }
        let mut statement = self.conn.prepare(
            "SELECT r.id FROM resources r JOIN files f ON f.resource_id = r.id
             WHERE r.project_id = ?1 AND r.namespace = 'file'
               AND (f.scan_id IS NULL OR f.scan_id != ?2)",
        )?;
        let ids = statement
            .query_map(params![project_id, scan_id], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        for id in &ids {
            self.conn.execute(
                "UPDATE resources SET current_version_id = NULL WHERE id = ?1
                   OR id IN (SELECT resource_id FROM symbols WHERE file_resource_id = ?1)",
                params![id],
            )?;
            self.conn
                .execute("DELETE FROM resources WHERE id = ?1", params![id])?;
        }
        Ok(ids.len())
    }

    pub fn replace_code_snapshot(&mut self, input: &CodeSnapshotInput<'_>) -> Result<i64> {
        if input.extractor.is_empty() || input.file.content_hash.is_none() {
            bail!("code snapshots require extractor and content_hash");
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = replace_code_snapshot_tx(&tx, input);
        match result {
            Ok(file_id) => {
                tx.commit()?;
                Ok(file_id)
            }
            Err(err) => Err(err),
        }
    }

    pub fn ensure_resource(&self, input: &ResourceInput<'_>) -> Result<i64> {
        validate_resource(input)?;
        let metadata = serde_json::to_string(input.metadata)?;
        let now = now_epoch_seconds();
        self.conn.execute(
            "INSERT INTO resources (
                project_id, namespace, external_id, canonical_uri, kind, title, description,
                origin_kind, origin_id, authority, status, stale_after, created_at, updated_at,
                metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?14)
             ON CONFLICT(project_id, canonical_uri) DO UPDATE SET
                namespace = excluded.namespace,
                external_id = excluded.external_id,
                kind = excluded.kind,
                title = excluded.title,
                description = excluded.description,
                origin_kind = excluded.origin_kind,
                origin_id = excluded.origin_id,
                authority = excluded.authority,
                status = excluded.status,
                stale_after = excluded.stale_after,
                updated_at = excluded.updated_at,
                metadata_json = excluded.metadata_json",
            params![
                input.project_id,
                input.namespace,
                input.external_id,
                input.canonical_uri,
                input.kind,
                input.title,
                input.description,
                input.origin_kind,
                input.origin_id,
                input.authority,
                input.status.unwrap_or("stable"),
                input.stale_after,
                now,
                metadata,
            ],
        )?;
        self.conn
            .query_row(
                "SELECT id FROM resources WHERE project_id = ?1 AND canonical_uri = ?2",
                params![input.project_id, input.canonical_uri],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn put_version(&self, input: &ResourceVersionInput<'_>) -> Result<i64> {
        if input.revision.is_empty()
            || input.source_format.is_empty()
            || input.content_hash.is_empty()
        {
            bail!("revision, source_format, and content_hash must be non-empty");
        }
        let existing = self
            .conn
            .query_row(
                "SELECT id FROM resource_versions WHERE resource_id = ?1 AND content_hash = ?2",
                params![input.resource_id, input.content_hash],
                |row| row.get(0),
            )
            .optional()?;
        let version_id = if let Some(id) = existing {
            id
        } else {
            self.conn.execute(
                "INSERT INTO resource_versions (
                    resource_id, revision, source_format, media_type, body, raw_metadata,
                    content_hash, generated_by, generated_at, indexed_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    input.resource_id,
                    input.revision,
                    input.source_format,
                    input.media_type,
                    input.body,
                    input.raw_metadata,
                    input.content_hash,
                    input.generated_by,
                    input.generated_at,
                    now_epoch_seconds(),
                ],
            )?;
            self.conn.last_insert_rowid()
        };
        self.conn.execute(
            "UPDATE resources SET current_version_id = ?1, updated_at = ?2 WHERE id = ?3",
            params![version_id, now_epoch_seconds(), input.resource_id],
        )?;
        Ok(version_id)
    }

    pub fn replace_segments(
        &mut self,
        resource_version_id: i64,
        segments: &[ContentSegmentInput<'_>],
    ) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM content_segments WHERE resource_version_id = ?1",
            params![resource_version_id],
        )?;
        {
            let mut statement = tx.prepare_cached(
                "INSERT INTO content_segments (
                    resource_version_id, ordinal, title, heading_path, text, start_line, end_line,
                    start_byte, end_byte, token_count, content_hash, metadata_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for segment in segments {
                statement.execute(params![
                    resource_version_id,
                    to_i64(segment.ordinal)?,
                    segment.title,
                    segment.heading_path,
                    segment.text,
                    optional_usize(segment.start_line)?,
                    optional_usize(segment.end_line)?,
                    optional_usize(segment.start_byte)?,
                    optional_usize(segment.end_byte)?,
                    optional_usize(segment.token_count)?,
                    segment.content_hash,
                    serde_json::to_string(segment.metadata)?,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn replace_edges_for_source(
        &mut self,
        source_version_id: i64,
        extractor: &str,
        edges: &[EdgeInput<'_>],
    ) -> Result<()> {
        if extractor.is_empty() {
            bail!("extractor must be non-empty");
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "DELETE FROM edges WHERE source_version_id = ?1 AND extractor = ?2",
            params![source_version_id, extractor],
        )?;
        {
            let mut statement = tx.prepare_cached(
                "INSERT INTO edges (
                    src_resource_id, dst_resource_id, dst_ref, relation, confidence, extractor,
                    source_resource_id, source_version_id, start_line, end_line, start_byte,
                    end_byte, content_hash, metadata_json
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            )?;
            for edge in edges {
                validate_edge(edge, extractor)?;
                statement.execute(params![
                    edge.src_resource_id,
                    edge.dst_resource_id,
                    edge.dst_ref,
                    edge.relation,
                    edge.confidence,
                    edge.extractor,
                    edge.source_resource_id,
                    source_version_id,
                    optional_usize(edge.start_line)?,
                    optional_usize(edge.end_line)?,
                    optional_usize(edge.start_byte)?,
                    optional_usize(edge.end_byte)?,
                    edge.content_hash,
                    serde_json::to_string(edge.metadata)?,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn add_tag(&self, resource_id: i64, tag: &str) -> Result<()> {
        if tag.trim().is_empty() {
            bail!("tag must be non-empty");
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO tags (resource_id, tag) VALUES (?1, ?2)",
            params![resource_id, tag],
        )?;
        Ok(())
    }

    pub fn add_alias(
        &self,
        project_id: &str,
        resource_id: i64,
        alias_uri: &str,
        reason: Option<&str>,
    ) -> Result<()> {
        if project_id.is_empty() || alias_uri.is_empty() {
            bail!("project_id and alias_uri must be non-empty");
        }
        self.conn.execute(
            "INSERT INTO resource_aliases (project_id, resource_id, alias_uri, reason)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(project_id, alias_uri) DO UPDATE SET
                resource_id = excluded.resource_id, reason = excluded.reason",
            params![project_id, resource_id, alias_uri, reason],
        )?;
        Ok(())
    }

    pub fn add_provenance(&self, input: &ProvenanceInput<'_>) -> Result<()> {
        if input.source_ref.is_empty() {
            bail!("source_ref must be non-empty");
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO provenance (
                resource_version_id, source_resource_id, source_ref, author, usage_count,
                last_modified, metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                input.resource_version_id,
                input.source_resource_id,
                input.source_ref,
                input.author,
                input.usage_count.map(i64::try_from).transpose()?,
                input.last_modified,
                serde_json::to_string(input.metadata)?,
            ],
        )?;
        Ok(())
    }

    pub fn add_verification(&self, input: &VerificationInput<'_>) -> Result<()> {
        if input.verification_kind.is_empty() {
            bail!("verification_kind must be non-empty");
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO verifications (
                resource_version_id, actor_resource_id, verified_at, verification_kind,
                metadata_json
             ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                input.resource_version_id,
                input.actor_resource_id,
                input.verified_at,
                input.verification_kind,
                serde_json::to_string(input.metadata)?,
            ],
        )?;
        Ok(())
    }

    pub fn resource_by_uri(&self, project_id: &str, uri: &str) -> Result<Option<ResourceMatch>> {
        self.conn
            .query_row(
                "SELECT id, canonical_uri, namespace, kind, title, authority, origin_kind,
                        origin_id, status, current_version_id
                 FROM resources WHERE project_id = ?1 AND canonical_uri = ?2",
                params![project_id, uri],
                row_to_resource,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn search_segments(&self, query: &str, limit: usize) -> Result<Vec<SearchMatch>> {
        if query.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT r.id, r.canonical_uri, r.namespace, r.kind, r.title, r.authority,
                    r.origin_kind, r.origin_id, r.status, r.current_version_id,
                    s.id, s.heading_path, s.text, bm25(content_segments_fts)
             FROM content_segments_fts
             JOIN content_segments s ON s.id = content_segments_fts.rowid
             JOIN resource_versions v ON v.id = s.resource_version_id
             JOIN resources r ON r.id = v.resource_id
             WHERE content_segments_fts MATCH ?1 AND r.current_version_id = v.id
             ORDER BY bm25(content_segments_fts), r.canonical_uri, s.ordinal
             LIMIT ?2",
        )?;
        let rows = statement.query_map(params![query, i64::try_from(limit)?], |row| {
            let rank: f64 = row.get(13)?;
            Ok(SearchMatch {
                resource: ResourceMatch {
                    id: row.get(0)?,
                    canonical_uri: row.get(1)?,
                    namespace: row.get(2)?,
                    kind: row.get(3)?,
                    title: row.get(4)?,
                    authority: row.get(5)?,
                    origin_kind: row.get(6)?,
                    origin_id: row.get(7)?,
                    status: row.get(8)?,
                    current_version_id: row.get(9)?,
                },
                segment_id: row.get(10)?,
                heading_path: row.get(11)?,
                text: row.get(12)?,
                rank_micros: (rank * 1_000_000.0) as i64,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn search_segments_filtered(
        &self,
        project_id: &str,
        query: &str,
        filter: &SearchFilter<'_>,
        limit: usize,
    ) -> Result<Vec<SearchMatch>> {
        if project_id.is_empty() || query.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let path_pattern = filter.path.map(|path| format!("%{path}%"));
        // Two shaping rules on top of relevance:
        //
        // 1. One row per resource — its best-matching segment. Without this a
        //    single document floods the page with its own sections.
        // 2. Authority before relevance. Derived concepts synthesized from code
        //    outnumber authored knowledge by orders of magnitude, so a purely
        //    relevance-ordered page buries what the repository actually asserts.
        //    Callers that want only derived rows filter with `origin`.
        let mut statement = self.conn.prepare(
            "SELECT id, canonical_uri, namespace, kind, title, authority,
                    origin_kind, origin_id, status, current_version_id,
                    segment_id, heading_path, text, rank
             FROM (
             SELECT *, ROW_NUMBER() OVER (
                        PARTITION BY id ORDER BY rank, ordinal, segment_id
                    ) AS segment_rank
             FROM (
             SELECT r.id AS id, r.canonical_uri AS canonical_uri, r.namespace AS namespace,
                    r.kind AS kind, r.title AS title, r.authority AS authority,
                    r.origin_kind AS origin_kind, r.origin_id AS origin_id, r.status AS status,
                    r.current_version_id AS current_version_id,
                    s.id AS segment_id, s.heading_path AS heading_path, s.text AS text,
                    s.ordinal AS ordinal, bm25(content_segments_fts) AS rank
             FROM content_segments_fts
             JOIN content_segments s ON s.id = content_segments_fts.rowid
             JOIN resource_versions v ON v.id = s.resource_version_id
             JOIN resources r ON r.id = v.resource_id
             LEFT JOIN files f ON f.resource_id = r.id
             LEFT JOIN symbols sy ON sy.resource_id = r.id
             WHERE content_segments_fts MATCH ?2 AND r.current_version_id = v.id
               AND r.project_id = ?1
               AND (?3 IS NULL OR r.namespace = ?3)
               AND (?4 IS NULL OR r.kind = ?4)
               AND (?5 IS NULL OR r.status = ?5)
               AND (?6 IS NULL OR r.origin_id = ?6 OR r.origin_kind = ?6)
               AND (?7 IS NULL OR f.path LIKE ?7
                    OR EXISTS (SELECT 1 FROM symbols ps JOIN files pf ON pf.resource_id = ps.file_resource_id
                               WHERE ps.resource_id = r.id AND pf.path LIKE ?7))
               AND (?8 IS NULL OR f.language = ?8 OR sy.language = ?8)
               AND (?9 IS NULL OR EXISTS (
                    SELECT 1 FROM edges e
                    JOIN resources producer ON producer.id = e.source_resource_id
                    WHERE producer.current_version_id = e.source_version_id
                    AND (e.src_resource_id = r.id OR e.dst_resource_id = r.id)
                    AND e.relation = ?9))
             ))
             WHERE segment_rank = 1
             ORDER BY CASE authority WHEN 'repository' THEN 0 WHEN 'gateway' THEN 1 ELSE 2 END,
                      CASE status WHEN 'stable' THEN 0 WHEN 'draft' THEN 1 ELSE 2 END,
                      rank, canonical_uri
             LIMIT ?10",
        )?;
        let rows = statement.query_map(
            params![
                project_id,
                query,
                filter.namespace,
                filter.kind,
                filter.status,
                filter.origin,
                path_pattern,
                filter.language,
                filter.relation,
                i64::try_from(limit)?
            ],
            |row| {
                let rank: f64 = row.get(13)?;
                Ok(SearchMatch {
                    resource: ResourceMatch {
                        id: row.get(0)?,
                        canonical_uri: row.get(1)?,
                        namespace: row.get(2)?,
                        kind: row.get(3)?,
                        title: row.get(4)?,
                        authority: row.get(5)?,
                        origin_kind: row.get(6)?,
                        origin_id: row.get(7)?,
                        status: row.get(8)?,
                        current_version_id: row.get(9)?,
                    },
                    segment_id: row.get(10)?,
                    heading_path: row.get(11)?,
                    text: row.get(12)?,
                    rank_micros: (rank * 1_000_000.0) as i64,
                })
            },
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn resource_detail(&self, resource_id: i64) -> Result<Option<ResourceDetail>> {
        let detail = self
            .conn
            .query_row(
                "SELECT r.id, r.canonical_uri, r.namespace, r.kind, r.title, r.authority,
                        r.origin_kind, r.origin_id, r.status, r.current_version_id,
                        r.description, r.stale_after, r.metadata_json,
                        v.revision, v.source_format, v.content_hash, v.generated_by, v.generated_at,
                        (SELECT COUNT(*) FROM provenance p WHERE p.resource_version_id = v.id),
                        (SELECT COUNT(*) FROM verifications x WHERE x.resource_version_id = v.id)
                 FROM resources r LEFT JOIN resource_versions v ON v.id = r.current_version_id
                 WHERE r.id = ?1",
                params![resource_id],
                |row| {
                    let metadata: String = row.get(12)?;
                    Ok(ResourceDetail {
                        resource: ResourceMatch {
                            id: row.get(0)?,
                            canonical_uri: row.get(1)?,
                            namespace: row.get(2)?,
                            kind: row.get(3)?,
                            title: row.get(4)?,
                            authority: row.get(5)?,
                            origin_kind: row.get(6)?,
                            origin_id: row.get(7)?,
                            status: row.get(8)?,
                            current_version_id: row.get(9)?,
                        },
                        description: row.get(10)?,
                        stale_after: row.get(11)?,
                        metadata: serde_json::from_str(&metadata).unwrap_or(Value::Null),
                        revision: row.get(13)?,
                        source_format: row.get(14)?,
                        content_hash: row.get(15)?,
                        generated_by: row.get(16)?,
                        generated_at: row.get(17)?,
                        tags: Vec::new(),
                        provenance_count: usize::try_from(row.get::<_, i64>(18)?).unwrap_or(0),
                        verification_count: usize::try_from(row.get::<_, i64>(19)?).unwrap_or(0),
                    })
                },
            )
            .optional()?;
        let Some(mut detail) = detail else {
            return Ok(None);
        };
        let mut tags = self
            .conn
            .prepare("SELECT tag FROM tags WHERE resource_id = ?1 ORDER BY tag")?;
        detail.tags = tags
            .query_map(params![resource_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(detail))
    }

    /// Record that a tool touched a resource.
    ///
    /// Counts accumulate per (resource, tool) pair. Callers treat failure as
    /// nothing happened — an access signal is never worth failing a read over.
    pub fn record_access(&self, resource_id: i64, tool: &str) -> Result<()> {
        if tool.is_empty() {
            bail!("tool must be non-empty");
        }
        let now = now_epoch_seconds();
        self.conn.execute(
            "INSERT INTO resource_access (resource_id, tool, access_count, first_access, last_access)
             VALUES (?1, ?2, 1, ?3, ?3)
             ON CONFLICT(resource_id, tool) DO UPDATE SET
                access_count = resource_access.access_count + 1,
                last_access = excluded.last_access",
            params![resource_id, tool, now],
        )?;
        Ok(())
    }

    /// Record an access against a repository path, if that path is indexed.
    ///
    /// Returns whether a resource was found; unindexed paths are not an error.
    pub fn record_path_access(&self, path: &str, tool: &str) -> Result<bool> {
        let resource_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT resource_id FROM files WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        match resource_id {
            Some(id) => {
                self.record_access(id, tool)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Resources touched since `since`, most-used first.
    ///
    /// This is what gives an outcome concept its edges: the work that produced
    /// it is described by what it read.
    pub fn recent_accesses(
        &self,
        project_id: &str,
        since: i64,
        limit: usize,
    ) -> Result<Vec<(ResourceMatch, i64)>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let mut statement = self.conn.prepare(
            "SELECT r.id, r.canonical_uri, r.namespace, r.kind, r.title, r.authority,
                    r.origin_kind, r.origin_id, r.status, r.current_version_id,
                    SUM(a.access_count) AS total
             FROM resource_access a JOIN resources r ON r.id = a.resource_id
             WHERE r.project_id = ?1 AND a.last_access >= ?2
             GROUP BY r.id
             ORDER BY total DESC, r.canonical_uri
             LIMIT ?3",
        )?;
        let rows = statement
            .query_map(params![project_id, since, i64::try_from(limit)?], |row| {
                Ok((row_to_resource(row)?, row.get::<_, i64>(10)?))
            })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Total accesses recorded against a resource, across tools.
    pub fn access_count(&self, resource_id: i64) -> Result<i64> {
        Ok(self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(access_count), 0) FROM resource_access WHERE resource_id = ?1",
                params![resource_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    /// Look up a resource id by its namespace-scoped external identity.
    pub fn resource_id_by_external_id(
        &self,
        project_id: &str,
        namespace: &str,
        external_id: &str,
    ) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM resources
                 WHERE project_id = ?1 AND namespace = ?2 AND external_id = ?3",
                params![project_id, namespace, external_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Drop all but the `keep` most recent resources of one namespace/origin.
    ///
    /// Observations accumulate with use, so they need a ceiling; authored and
    /// synthesized origins are untouched because pruning is origin-scoped.
    pub fn prune_origin_to_recent(
        &mut self,
        project_id: &str,
        namespace: &str,
        origin_id: &str,
        keep: usize,
    ) -> Result<usize> {
        let retained: BTreeSet<String> = {
            let mut statement = self.conn.prepare(
                "SELECT external_id FROM resources
                 WHERE project_id = ?1 AND namespace = ?2 AND origin_id = ?3
                 ORDER BY updated_at DESC, id DESC
                 LIMIT ?4",
            )?;
            let rows = statement.query_map(
                params![project_id, namespace, origin_id, i64::try_from(keep)?],
                |row| row.get::<_, String>(0),
            )?;
            rows.collect::<rusqlite::Result<BTreeSet<_>>>()?
        };
        self.prune_origin_resources(project_id, namespace, origin_id, &retained)
    }

    /// Every current document stored for one namespace/origin pair, by identity.
    ///
    /// This is what materializes a stored bundle back onto disk.
    pub fn origin_documents(
        &self,
        project_id: &str,
        namespace: &str,
        origin_id: &str,
    ) -> Result<Vec<(String, String)>> {
        let ids: Vec<(i64, String)> = {
            let mut statement = self.conn.prepare(
                "SELECT id, external_id FROM resources
                 WHERE project_id = ?1 AND namespace = ?2 AND origin_id = ?3
                 ORDER BY external_id",
            )?;
            let rows = statement.query_map(params![project_id, namespace, origin_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut documents = Vec::with_capacity(ids.len());
        for (id, external_id) in ids {
            if let Some(document) = self.resource_document(id)? {
                documents.push((external_id, document));
            }
        }
        Ok(documents)
    }

    /// Reassemble a resource's current version as a Markdown document.
    ///
    /// Concepts are stored as frontmatter plus body, so this returns exactly
    /// what `okf export` would write for the same resource — the index is the
    /// document, not a cache of one.
    pub fn resource_document(&self, resource_id: i64) -> Result<Option<String>> {
        let stored: Option<(Option<String>, Option<String>)> = self
            .conn
            .query_row(
                "SELECT v.raw_metadata, v.body
                 FROM resources r JOIN resource_versions v ON v.id = r.current_version_id
                 WHERE r.id = ?1",
                params![resource_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((raw_metadata, body)) = stored else {
            return Ok(None);
        };
        let body = body.unwrap_or_default();
        Ok(Some(match raw_metadata {
            Some(frontmatter) => {
                let frontmatter = frontmatter.trim_end_matches('\n');
                format!("---\n{frontmatter}\n---\n{body}")
            }
            None => body,
        }))
    }

    pub fn edges_from(&self, resource_id: i64, relation: Option<&str>) -> Result<Vec<EdgeMatch>> {
        let mut statement = self.conn.prepare(
            "SELECT e.id, e.src_resource_id, e.dst_resource_id, e.dst_ref, e.relation,
                    e.confidence, e.extractor, e.source_version_id
             FROM edges e
             JOIN resources producer ON producer.id = e.source_resource_id
                AND producer.current_version_id = e.source_version_id
             WHERE e.src_resource_id = ?1 AND (?2 IS NULL OR e.relation = ?2)
             ORDER BY e.relation, COALESCE(e.dst_resource_id, 0), COALESCE(e.dst_ref, ''), e.id",
        )?;
        let rows = statement.query_map(params![resource_id, relation], |row| {
            Ok(EdgeMatch {
                id: row.get(0)?,
                src_resource_id: row.get(1)?,
                dst_resource_id: row.get(2)?,
                dst_ref: row.get(3)?,
                relation: row.get(4)?,
                confidence: row.get(5)?,
                extractor: row.get(6)?,
                source_version_id: row.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn find_resources(
        &self,
        project_id: &str,
        query: &str,
        namespace: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ResourceMatch>> {
        if query.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        let pattern = format!("%{query}%");
        let mut statement = self.conn.prepare(
            "SELECT id, canonical_uri, namespace, kind, title, authority, origin_kind,
                    origin_id, status, current_version_id
             FROM resources
             WHERE project_id = ?1
               AND (?2 IS NULL OR namespace = ?2)
               AND (canonical_uri = ?3 OR title LIKE ?4 OR external_id LIKE ?4)
             ORDER BY CASE WHEN canonical_uri = ?3 THEN 0
                           WHEN title = ?3 THEN 1
                           WHEN external_id = ?3 THEN 2 ELSE 3 END,
                      canonical_uri
             LIMIT ?5",
        )?;
        let rows = statement.query_map(
            params![project_id, namespace, query, pattern, i64::try_from(limit)?],
            row_to_resource,
        )?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn current_content_hash(&self, resource_id: i64) -> Result<Option<String>> {
        self.conn
            .query_row(
                "SELECT rv.content_hash FROM resources r
                 JOIN resource_versions rv ON rv.id = r.current_version_id
                 WHERE r.id = ?1",
                params![resource_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    /// Record that a producer has processed a file at a given input hash.
    ///
    /// `replace_code_snapshot` does this for the extractor that owns the
    /// snapshot; producers that derive further state from the same file (such
    /// as concept synthesis) record their own progress here so they can be
    /// gated independently of the extractor.
    pub fn mark_producer_state(&self, path: &str, producer: &str, input_hash: &str) -> Result<()> {
        if path.is_empty() || producer.is_empty() || input_hash.is_empty() {
            bail!("path, producer, and input_hash must be non-empty");
        }
        let file_id: i64 = self
            .conn
            .query_row(
                "SELECT resource_id FROM files WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?
            .with_context(|| format!("no indexed file at {path}"))?;
        self.conn.execute(
            "INSERT INTO producer_state (source_resource_id, producer, input_hash, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(source_resource_id, producer) DO UPDATE SET
                input_hash = excluded.input_hash, updated_at = excluded.updated_at",
            params![file_id, producer, input_hash, now_epoch_seconds()],
        )?;
        Ok(())
    }

    /// List every external id currently stored for one namespace/origin pair.
    ///
    /// Incremental producers skip unchanged inputs, so they cannot rebuild the
    /// retained set from what they just wrote; they reconcile against this.
    pub fn origin_external_ids(
        &self,
        project_id: &str,
        namespace: &str,
        origin_id: &str,
    ) -> Result<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT external_id FROM resources
             WHERE project_id = ?1 AND namespace = ?2 AND origin_id = ?3
             ORDER BY external_id",
        )?;
        let rows = statement.query_map(params![project_id, namespace, origin_id], |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn prune_origin_resources(
        &mut self,
        project_id: &str,
        namespace: &str,
        origin_id: &str,
        retained_external_ids: &BTreeSet<String>,
    ) -> Result<usize> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut statement = tx.prepare(
            "SELECT id, external_id FROM resources
             WHERE project_id = ?1 AND namespace = ?2 AND origin_id = ?3",
        )?;
        let candidates = statement
            .query_map(params![project_id, namespace, origin_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let removed: Vec<i64> = candidates
            .into_iter()
            .filter_map(|(id, external_id)| {
                (!retained_external_ids.contains(&external_id)).then_some(id)
            })
            .collect();
        for id in &removed {
            tx.execute(
                "UPDATE resources SET current_version_id = NULL WHERE id = ?1",
                params![id],
            )?;
            tx.execute("DELETE FROM resources WHERE id = ?1", params![id])?;
        }
        tx.commit()?;
        Ok(removed.len())
    }

    pub fn resolve_code_edges(
        &mut self,
        project_id: &str,
        extractor: &str,
    ) -> Result<ResolutionStats> {
        if project_id.is_empty() || extractor.is_empty() {
            bail!("project_id and extractor must be non-empty");
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = resolve_code_edges_tx(&tx, project_id, extractor);
        match result {
            Ok(stats) => {
                tx.commit()?;
                Ok(stats)
            }
            Err(err) => Err(err),
        }
    }

    pub fn traverse(
        &self,
        start_resource_id: i64,
        relation: Option<&str>,
        direction: &str,
        max_depth: usize,
        limit: usize,
    ) -> Result<Vec<TraversedEdge>> {
        if !["in", "out", "both"].contains(&direction) {
            bail!("direction must be in, out, or both");
        }
        if max_depth == 0 || limit == 0 {
            return Ok(Vec::new());
        }
        let mut queue = VecDeque::from([(start_resource_id, 0usize)]);
        let mut visited_nodes = BTreeSet::from([start_resource_id]);
        let mut visited_edges = BTreeSet::new();
        let mut output = Vec::new();

        while let Some((resource_id, depth)) = queue.pop_front() {
            if depth >= max_depth || output.len() >= limit {
                continue;
            }
            let edges = graph_edges_for_resource(&self.conn, resource_id, relation, direction)?;
            for mut edge in edges {
                if output.len() >= limit || !visited_edges.insert(edge.id) {
                    continue;
                }
                edge.depth = depth + 1;
                let next_id = graph_next_resource_id(&self.conn, edge.id, resource_id)?;
                output.push(edge);
                if let Some(next_id) = next_id {
                    if visited_nodes.insert(next_id) {
                        queue.push_back((next_id, depth + 1));
                    }
                }
            }
        }
        Ok(output)
    }

    pub fn migrate_legacy_indexes(
        &mut self,
        project_id: &str,
        project_root: &Path,
        files_db: Option<&Path>,
        symbols_db: Option<&Path>,
    ) -> Result<LegacyMigrationStats> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result =
            migrate_legacy_transaction(&tx, project_id, project_root, files_db, symbols_db);
        match result {
            Ok(stats) => {
                tx.commit()?;
                Ok(stats)
            }
            Err(err) => Err(err),
        }
    }

    #[cfg(test)]
    fn count(&self, table: &str) -> Result<usize> {
        let allowed = [
            "resources",
            "resource_versions",
            "content_segments",
            "edges",
            "files",
            "symbols",
            "tags",
            "provenance",
            "verifications",
            "resource_aliases",
            "resource_access",
        ];
        if !allowed.contains(&table) {
            bail!("unsupported count table");
        }
        let sql = format!("SELECT COUNT(*) FROM {table}");
        let count: i64 = self.conn.query_row(&sql, [], |row| row.get(0))?;
        Ok(usize::try_from(count)?)
    }
}

fn validate_resource(input: &ResourceInput<'_>) -> Result<()> {
    for (name, value) in [
        ("project_id", input.project_id),
        ("namespace", input.namespace),
        ("external_id", input.external_id),
        ("canonical_uri", input.canonical_uri),
        ("kind", input.kind),
        ("title", input.title),
        ("origin_id", input.origin_id),
    ] {
        if value.trim().is_empty() {
            bail!("{name} must be non-empty");
        }
    }
    if !["repository", "local-derived", "gateway", "external"].contains(&input.origin_kind) {
        bail!("invalid origin_kind: {}", input.origin_kind);
    }
    if !["repository", "gateway", "derived"].contains(&input.authority) {
        bail!("invalid authority: {}", input.authority);
    }
    if !["draft", "stable", "deprecated"].contains(&input.status.unwrap_or("stable")) {
        bail!("invalid status: {}", input.status.unwrap_or("stable"));
    }
    Ok(())
}

fn validate_edge(edge: &EdgeInput<'_>, extractor: &str) -> Result<()> {
    if edge.relation.is_empty() || edge.content_hash.is_empty() {
        bail!("edge relation and content_hash must be non-empty");
    }
    if edge.extractor != extractor {
        bail!("edge extractor does not match replacement owner");
    }
    if edge.dst_resource_id.is_none() && edge.dst_ref.is_none() {
        bail!("edge must have a destination resource or unresolved reference");
    }
    if !["extracted", "resolved", "inferred", "ambiguous"].contains(&edge.confidence) {
        bail!("invalid edge confidence: {}", edge.confidence);
    }
    Ok(())
}

#[derive(Debug)]
struct UnresolvedCodeEdge {
    id: i64,
    dst_ref: String,
    relation: String,
    confidence: String,
    source_path: String,
    language: String,
}

#[derive(Debug)]
struct ResolutionCandidate {
    id: i64,
    name: String,
    kind: String,
    path: String,
    score: i64,
}

fn resolve_code_edges_tx(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    extractor: &str,
) -> Result<ResolutionStats> {
    let mut statement = tx.prepare(
        "SELECT e.id, e.dst_ref, e.relation, e.confidence, f.path, f.language
         FROM edges e
         JOIN files f ON f.resource_id = e.source_resource_id
         JOIN resources r ON r.id = f.resource_id
         WHERE r.project_id = ?1 AND r.current_version_id = e.source_version_id
           AND e.extractor = ?2 AND e.dst_ref IS NOT NULL
         ORDER BY f.path, e.start_byte, e.id",
    )?;
    let edges = statement
        .query_map(params![project_id, extractor], |row| {
            Ok(UnresolvedCodeEdge {
                id: row.get(0)?,
                dst_ref: row.get(1)?,
                relation: row.get(2)?,
                confidence: row.get(3)?,
                source_path: row.get(4)?,
                language: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    let mut stats = ResolutionStats::default();
    for edge in edges {
        stats.edges_seen += 1;
        let mut candidates = if edge.relation == "imports" {
            import_candidates(tx, project_id, &edge)?
        } else {
            symbol_candidates(tx, project_id, &edge)?
        };
        candidates.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.id.cmp(&right.id))
        });
        let best = candidates.first();
        let unique_best = best.is_some_and(|best| {
            candidates
                .get(1)
                .is_none_or(|second| second.score < best.score)
        });
        if let Some(best) = best.filter(|_| unique_best) {
            let relation = corrected_relation(&edge.relation, &edge.language, &best.kind);
            tx.execute(
                "UPDATE edges SET dst_resource_id = ?1, relation = ?2, confidence = 'resolved'
                 WHERE id = ?3",
                params![best.id, relation, edge.id],
            )?;
            stats.resolved += 1;
        } else if best.is_some() {
            tx.execute(
                "UPDATE edges SET dst_resource_id = NULL, confidence = 'ambiguous' WHERE id = ?1",
                params![edge.id],
            )?;
            stats.ambiguous += 1;
        } else {
            tx.execute(
                "UPDATE edges SET dst_resource_id = NULL, confidence = ?1 WHERE id = ?2",
                params![edge.confidence, edge.id],
            )?;
            stats.unresolved += 1;
        }
    }
    Ok(stats)
}

fn symbol_candidates(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    edge: &UnresolvedCodeEdge,
) -> Result<Vec<ResolutionCandidate>> {
    let (qualifier, target) = split_target_reference(&edge.dst_ref);
    if target.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = tx.prepare(
        "SELECT s.resource_id, s.name, s.symbol_kind, f.path
         FROM symbols s
         JOIN files f ON f.resource_id = s.file_resource_id
         JOIN resources r ON r.id = s.resource_id
         WHERE r.project_id = ?1 AND f.language = ?2 AND lower(s.name) = lower(?3)
         ORDER BY f.path, s.start_line, s.resource_id",
    )?;
    let rows = statement.query_map(params![project_id, edge.language, target], |row| {
        Ok(ResolutionCandidate {
            id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            path: row.get(3)?,
            score: 0,
        })
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        let mut candidate = row?;
        if edge.relation != "calls"
            && matches!(candidate.kind.as_str(), "fn" | "method" | "var" | "prop")
        {
            continue;
        }
        if candidate.name == target {
            candidate.score += 20;
        }
        if candidate.path == edge.source_path {
            candidate.score += 100;
        }
        if let Some(qualifier) = qualifier {
            if path_matches_qualifier(&candidate.path, qualifier) {
                candidate.score += 60;
            }
        }
        candidates.push(candidate);
    }
    Ok(candidates)
}

fn import_candidates(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    edge: &UnresolvedCodeEdge,
) -> Result<Vec<ResolutionCandidate>> {
    let normalized = normalize_import_reference(&edge.dst_ref);
    let source_parent = Path::new(&edge.source_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let relative_target = source_parent
        .join(normalized.trim_start_matches("./"))
        .to_string_lossy()
        .replace('\\', "/");
    let mut statement = tx.prepare(
        "SELECT f.resource_id, f.path
         FROM files f JOIN resources r ON r.id = f.resource_id
         WHERE r.project_id = ?1 ORDER BY f.path",
    )?;
    let rows = statement.query_map(params![project_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut candidates = Vec::new();
    for row in rows {
        let (id, path) = row?;
        let without_extension = strip_known_extension(&path);
        let mut score = 0;
        if path == normalized || without_extension == normalized {
            score = 120;
        } else if path == relative_target || without_extension == relative_target {
            score = 110;
        } else if path.ends_with(&format!("/{normalized}"))
            || without_extension.ends_with(&format!("/{normalized}"))
        {
            score = 80;
        } else if Path::new(&path)
            .file_stem()
            .and_then(|value| value.to_str())
            == Path::new(&normalized)
                .file_name()
                .and_then(|value| value.to_str())
        {
            score = 40;
        }
        if score > 0 && path != edge.source_path {
            candidates.push(ResolutionCandidate {
                id,
                name: path.clone(),
                kind: "file".to_owned(),
                path,
                score,
            });
        }
    }

    if candidates.is_empty() && edge.dst_ref.contains("::") {
        let mut imported_symbols = symbol_candidates(tx, project_id, edge)?;
        for candidate in &mut imported_symbols {
            candidate.score += 30;
        }
        candidates.extend(imported_symbols);
    }
    Ok(candidates)
}

fn split_target_reference(raw: &str) -> (Option<&str>, &str) {
    let raw = raw.trim().trim_matches(['"', '\'', '<', '>']);
    for separator in ["::", "->", "."] {
        if let Some((qualifier, target)) = raw.rsplit_once(separator) {
            return (Some(qualifier), trim_generic_target(target));
        }
    }
    (None, trim_generic_target(raw))
}

fn trim_generic_target(target: &str) -> &str {
    target
        .split(['<', '(', '['])
        .next()
        .unwrap_or(target)
        .trim()
}

fn path_matches_qualifier(path: &str, qualifier: &str) -> bool {
    let qualifier = trim_rust_path_prefix(qualifier)
        .replace("::", "/")
        .replace('.', "/");
    let stemmed = strip_known_extension(path);
    stemmed.ends_with(&qualifier)
        || Path::new(path)
            .file_stem()
            .and_then(|value| value.to_str())
            .is_some_and(|stem| qualifier.ends_with(stem))
}

fn normalize_import_reference(raw: &str) -> String {
    let normalized =
        trim_rust_path_prefix(raw.trim().trim_matches(['"', '\'', '<', '>'])).replace("::", "/");

    if normalized.starts_with('.') || Path::new(&normalized).extension().is_some() {
        normalized
    } else {
        normalized.replace('.', "/")
    }
}

fn trim_rust_path_prefix(value: &str) -> &str {
    ["crate::", "self::", "super::"]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .unwrap_or(value)
}

fn strip_known_extension(path: &str) -> String {
    Path::new(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
}

fn corrected_relation<'a>(relation: &'a str, language: &str, target_kind: &str) -> &'a str {
    if language == "C#" && matches!(relation, "inherits" | "implements") {
        if target_kind == "interface" {
            "implements"
        } else {
            "inherits"
        }
    } else {
        relation
    }
}

fn graph_edges_for_resource(
    conn: &Connection,
    resource_id: i64,
    relation: Option<&str>,
    direction: &str,
) -> Result<Vec<TraversedEdge>> {
    let mut statement = conn.prepare(
        "SELECT e.id, e.src_resource_id, e.dst_resource_id, e.relation, e.confidence,
                sr.canonical_uri, sr.title, dr.canonical_uri, dr.title, e.dst_ref,
                f.path, e.start_line
         FROM edges e
         JOIN resources sr ON sr.id = e.src_resource_id
         JOIN resources producer ON producer.id = e.source_resource_id
            AND producer.current_version_id = e.source_version_id
         LEFT JOIN resources dr ON dr.id = e.dst_resource_id
         LEFT JOIN files f ON f.resource_id = e.source_resource_id
         WHERE (?2 IS NULL OR e.relation = ?2)
           AND ((?3 IN ('out', 'both') AND e.src_resource_id = ?1)
             OR (?3 IN ('in', 'both') AND e.dst_resource_id = ?1))
         ORDER BY e.relation, sr.canonical_uri, COALESCE(dr.canonical_uri, e.dst_ref), e.id",
    )?;
    let rows = statement.query_map(params![resource_id, relation, direction], |row| {
        let src_id: i64 = row.get(1)?;
        let line = row.get::<_, Option<i64>>(11)?;
        Ok(TraversedEdge {
            id: row.get(0)?,
            depth: 0,
            direction: if src_id == resource_id { "out" } else { "in" }.to_owned(),
            relation: row.get(3)?,
            confidence: row.get(4)?,
            source_uri: row.get(5)?,
            source_title: row.get(6)?,
            target_uri: row.get(7)?,
            target_title: row.get(8)?,
            unresolved_ref: row.get(9)?,
            source_path: row.get(10)?,
            start_line: line.and_then(|value| usize::try_from(value).ok()),
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn graph_next_resource_id(
    conn: &Connection,
    edge_id: i64,
    current_resource_id: i64,
) -> Result<Option<i64>> {
    let (source, target): (i64, Option<i64>) = conn.query_row(
        "SELECT src_resource_id, dst_resource_id FROM edges WHERE id = ?1",
        params![edge_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(if source == current_resource_id {
        target
    } else {
        Some(source)
    })
}

fn replace_code_snapshot_tx(
    tx: &rusqlite::Transaction<'_>,
    input: &CodeSnapshotInput<'_>,
) -> Result<i64> {
    let file_hash = input
        .file
        .content_hash
        .context("code snapshot content hash")?;
    let path = normalized_repo_path(input.file.project_root, input.file.path)?;
    let uri = canonical_repo_uri(
        input.file.project_id,
        input.file.project_root,
        input.file.path,
    )?;
    let empty_metadata = serde_json::json!({});
    let file_id = ensure_resource_tx(
        tx,
        &ResourceInput {
            project_id: input.file.project_id,
            namespace: "file",
            external_id: &path,
            canonical_uri: &uri,
            kind: "file",
            title: &path,
            description: None,
            origin_kind: "repository",
            origin_id: input.file.project_id,
            authority: "repository",
            status: None,
            stale_after: None,
            metadata: &empty_metadata,
        },
    )?;
    tx.execute(
        "INSERT INTO files (
            resource_id, path, language, extension, size, mtime_secs, mtime_nanos,
            content_hash, scan_id
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(resource_id) DO UPDATE SET
            path = excluded.path, language = excluded.language,
            extension = excluded.extension, size = excluded.size,
            mtime_secs = excluded.mtime_secs, mtime_nanos = excluded.mtime_nanos,
            content_hash = excluded.content_hash,
            scan_id = COALESCE(excluded.scan_id, files.scan_id)",
        params![
            file_id,
            path,
            input.file.language,
            input.file.extension,
            i64::try_from(input.file.size)?,
            input.file.mtime_secs,
            input.file.mtime_nanos,
            file_hash,
            input.file.scan_id,
        ],
    )?;
    let file_version_id = put_version_tx(
        tx,
        &ResourceVersionInput {
            resource_id: file_id,
            revision: file_hash,
            source_format: "source",
            media_type: Some("text/plain"),
            body: Some(input.source),
            raw_metadata: None,
            content_hash: file_hash,
            generated_by: None,
            generated_at: None,
        },
    )?;

    tx.execute(
        "DELETE FROM edges WHERE source_resource_id = ?1 AND extractor = ?2",
        params![file_id, input.extractor],
    )?;
    tx.execute(
        "UPDATE resources SET current_version_id = NULL
         WHERE id IN (SELECT resource_id FROM symbols WHERE file_resource_id = ?1)",
        params![file_id],
    )?;
    tx.execute(
        "DELETE FROM resources
         WHERE id IN (SELECT resource_id FROM symbols WHERE file_resource_id = ?1)",
        params![file_id],
    )?;

    let mut symbols_by_key = BTreeMap::new();
    for symbol in input.symbols {
        let symbol_uri = canonical_symbol_uri(input.file.project_id, &path, symbol.stable_key)?;
        let symbol_metadata = serde_json::json!({"extractor": input.extractor});
        let symbol_id = ensure_resource_tx(
            tx,
            &ResourceInput {
                project_id: input.file.project_id,
                namespace: "symbol",
                external_id: symbol.stable_key,
                canonical_uri: &symbol_uri,
                kind: &format!("symbol/{}", symbol.kind),
                title: symbol.name,
                description: None,
                origin_kind: "local-derived",
                origin_id: &uri,
                authority: "derived",
                status: None,
                stale_after: None,
                metadata: &symbol_metadata,
            },
        )?;
        let symbol_hash = format!("{file_hash}:{}", symbol.stable_key);
        put_version_tx(
            tx,
            &ResourceVersionInput {
                resource_id: symbol_id,
                revision: file_hash,
                source_format: input.extractor,
                media_type: None,
                body: None,
                raw_metadata: None,
                content_hash: &symbol_hash,
                generated_by: Some(input.extractor),
                generated_at: None,
            },
        )?;
        tx.execute(
            "INSERT INTO symbols (
                resource_id, file_resource_id, name, symbol_kind, parent_resource_id,
                stable_key, start_line, end_line, start_byte, end_byte, language
             ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                symbol_id,
                file_id,
                symbol.name,
                symbol.kind,
                symbol.stable_key,
                to_i64(symbol.start_line)?,
                to_i64(symbol.end_line)?,
                optional_usize(symbol.start_byte)?,
                optional_usize(symbol.end_byte)?,
                input.file.language.context("code snapshot language")?,
            ],
        )?;
        symbols_by_key.insert(symbol.stable_key, symbol_id);
    }

    for symbol in input.symbols {
        let Some(parent_key) = symbol.parent_stable_key else {
            continue;
        };
        let Some(parent_id) = symbols_by_key.get(parent_key) else {
            continue;
        };
        let symbol_id = symbols_by_key[symbol.stable_key];
        tx.execute(
            "UPDATE symbols SET parent_resource_id = ?1 WHERE resource_id = ?2",
            params![parent_id, symbol_id],
        )?;
    }

    for relationship in input.relationships {
        if relationship.dst_ref.is_empty() || relationship.relation.is_empty() {
            bail!("relationship destination and relation must be non-empty");
        }
        let src_resource_id = relationship
            .source_stable_key
            .and_then(|key| symbols_by_key.get(key).copied())
            .unwrap_or(file_id);
        tx.execute(
            "INSERT INTO edges (
                src_resource_id, dst_resource_id, dst_ref, relation, confidence, extractor,
                source_resource_id, source_version_id, start_line, end_line, start_byte,
                end_byte, content_hash, metadata_json
             ) VALUES (?1, NULL, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                src_resource_id,
                relationship.dst_ref,
                relationship.relation,
                relationship.confidence,
                input.extractor,
                file_id,
                file_version_id,
                to_i64(relationship.start_line)?,
                to_i64(relationship.end_line)?,
                to_i64(relationship.start_byte)?,
                to_i64(relationship.end_byte)?,
                relationship.content_hash,
                serde_json::to_string(relationship.metadata)?,
            ],
        )?;
    }
    tx.execute(
        "INSERT INTO producer_state (source_resource_id, producer, input_hash, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(source_resource_id, producer) DO UPDATE SET
            input_hash = excluded.input_hash, updated_at = excluded.updated_at",
        params![file_id, input.extractor, file_hash, now_epoch_seconds()],
    )?;
    Ok(file_id)
}

fn put_version_tx(tx: &rusqlite::Transaction<'_>, input: &ResourceVersionInput<'_>) -> Result<i64> {
    if input.revision.is_empty() || input.source_format.is_empty() || input.content_hash.is_empty()
    {
        bail!("revision, source_format, and content_hash must be non-empty");
    }
    tx.execute(
        "INSERT OR IGNORE INTO resource_versions (
            resource_id, revision, source_format, media_type, body, raw_metadata,
            content_hash, generated_by, generated_at, indexed_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            input.resource_id,
            input.revision,
            input.source_format,
            input.media_type,
            input.body,
            input.raw_metadata,
            input.content_hash,
            input.generated_by,
            input.generated_at,
            now_epoch_seconds(),
        ],
    )?;
    let version_id: i64 = tx.query_row(
        "SELECT id FROM resource_versions WHERE resource_id = ?1 AND content_hash = ?2",
        params![input.resource_id, input.content_hash],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE resources SET current_version_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![version_id, now_epoch_seconds(), input.resource_id],
    )?;
    Ok(version_id)
}

fn migrate_legacy_transaction(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    project_root: &Path,
    files_db: Option<&Path>,
    symbols_db: Option<&Path>,
) -> Result<LegacyMigrationStats> {
    let mut stats = LegacyMigrationStats::default();
    if let Some(path) = files_db.filter(|path| path.exists()) {
        let source = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("open legacy file index {}", path.display()))?;
        let mut statement = source.prepare(
            "SELECT path, extension, size, mtime_secs, content_hash FROM files ORDER BY path",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?;
        for row in rows {
            let (path, extension, size, mtime_secs, content_hash) = row?;
            stats.files_seen += 1;
            let resource_id = migrate_file_resource(
                tx,
                project_id,
                project_root,
                &path,
                Some(&extension),
                size,
                mtime_secs,
                0,
                content_hash.as_deref(),
            )?;
            stats.files_migrated += 1;
            if let Some(hash) = content_hash {
                migrate_legacy_version(tx, resource_id, &hash)?;
                stats.versions_migrated += 1;
            }
        }
    }

    if let Some(path) = symbols_db.filter(|path| path.exists()) {
        let source = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("open legacy symbol index {}", path.display()))?;
        let mut statement = source.prepare(
            "SELECT s.id, f.path, f.mtime_secs, f.mtime_nanos, s.name, s.kind,
                    s.start_line, s.end_line, s.language, s.parent
             FROM symbols s JOIN files f ON f.id = s.file_id
             ORDER BY f.path, s.start_line, s.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
            ))
        })?;
        for row in rows {
            let (
                legacy_id,
                path,
                mtime_secs,
                mtime_nanos,
                name,
                kind,
                start_line,
                end_line,
                language,
                parent,
            ) = row?;
            stats.symbols_seen += 1;
            let file_resource_id = migrate_file_resource(
                tx,
                project_id,
                project_root,
                &path,
                Path::new(&path)
                    .extension()
                    .and_then(|value| value.to_str()),
                0,
                mtime_secs,
                mtime_nanos,
                None,
            )?;
            let repo_path = normalized_repo_path(project_root, Path::new(&path))?;
            let stable_key = legacy_symbol_key(legacy_id, parent.as_deref(), &name, &kind);
            let uri = canonical_symbol_uri(project_id, &repo_path, &stable_key)?;
            let metadata = serde_json::json!({"legacy_symbol_id": legacy_id});
            let symbol_id = ensure_resource_tx(
                tx,
                &ResourceInput {
                    project_id,
                    namespace: "symbol",
                    external_id: &stable_key,
                    canonical_uri: &uri,
                    kind: &format!("symbol/{kind}"),
                    title: &name,
                    description: None,
                    origin_kind: "local-derived",
                    origin_id: "legacy-symbol-index",
                    authority: "derived",
                    status: None,
                    stale_after: None,
                    metadata: &metadata,
                },
            )?;
            tx.execute(
                "INSERT INTO symbols (
                    resource_id, file_resource_id, name, symbol_kind, parent_resource_id,
                    stable_key, start_line, end_line, start_byte, end_byte, language
                 ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, NULL, NULL, ?8)
                 ON CONFLICT(resource_id) DO UPDATE SET
                    file_resource_id = excluded.file_resource_id,
                    name = excluded.name,
                    symbol_kind = excluded.symbol_kind,
                    stable_key = excluded.stable_key,
                    start_line = excluded.start_line,
                    end_line = excluded.end_line,
                    language = excluded.language",
                params![
                    symbol_id,
                    file_resource_id,
                    name,
                    kind,
                    stable_key,
                    start_line,
                    end_line,
                    language,
                ],
            )?;
            stats.symbols_migrated += 1;
        }
    }
    Ok(stats)
}

#[allow(clippy::too_many_arguments)]
fn migrate_file_resource(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
    project_root: &Path,
    raw_path: &str,
    extension: Option<&str>,
    size: i64,
    mtime_secs: i64,
    mtime_nanos: i64,
    content_hash: Option<&str>,
) -> Result<i64> {
    let path = normalized_repo_path(project_root, Path::new(raw_path))?;
    let uri = canonical_repo_uri(project_id, project_root, Path::new(raw_path))?;
    let metadata = serde_json::json!({"legacy_path": raw_path});
    let resource_id = ensure_resource_tx(
        tx,
        &ResourceInput {
            project_id,
            namespace: "file",
            external_id: &path,
            canonical_uri: &uri,
            kind: "file",
            title: &path,
            description: None,
            origin_kind: "repository",
            origin_id: project_id,
            authority: "repository",
            status: None,
            stale_after: None,
            metadata: &metadata,
        },
    )?;
    tx.execute(
        "INSERT INTO files (
            resource_id, path, language, extension, size, mtime_secs, mtime_nanos, content_hash
         ) VALUES (?1, ?2, NULL, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(resource_id) DO UPDATE SET
            path = excluded.path,
            extension = CASE WHEN excluded.extension = '' THEN files.extension ELSE excluded.extension END,
            size = CASE WHEN excluded.size = 0 THEN files.size ELSE excluded.size END,
            mtime_secs = MAX(files.mtime_secs, excluded.mtime_secs),
            mtime_nanos = CASE WHEN excluded.mtime_secs >= files.mtime_secs
                              THEN excluded.mtime_nanos ELSE files.mtime_nanos END,
            content_hash = COALESCE(excluded.content_hash, files.content_hash)",
        params![
            resource_id,
            path,
            extension.unwrap_or(""),
            size,
            mtime_secs,
            mtime_nanos,
            content_hash,
        ],
    )?;
    Ok(resource_id)
}

fn migrate_legacy_version(
    tx: &rusqlite::Transaction<'_>,
    resource_id: i64,
    content_hash: &str,
) -> Result<i64> {
    tx.execute(
        "INSERT OR IGNORE INTO resource_versions (
            resource_id, revision, source_format, media_type, body, raw_metadata,
            content_hash, generated_by, generated_at, indexed_at
         ) VALUES (?1, ?2, 'legacy-file-index', NULL, NULL, NULL, ?2, NULL, NULL, ?3)",
        params![resource_id, content_hash, now_epoch_seconds()],
    )?;
    let version_id: i64 = tx.query_row(
        "SELECT id FROM resource_versions WHERE resource_id = ?1 AND content_hash = ?2",
        params![resource_id, content_hash],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE resources SET current_version_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![version_id, now_epoch_seconds(), resource_id],
    )?;
    Ok(version_id)
}

fn ensure_resource_tx(tx: &rusqlite::Transaction<'_>, input: &ResourceInput<'_>) -> Result<i64> {
    validate_resource(input)?;
    let metadata = serde_json::to_string(input.metadata)?;
    let now = now_epoch_seconds();
    tx.execute(
        "INSERT INTO resources (
            project_id, namespace, external_id, canonical_uri, kind, title, description,
            origin_kind, origin_id, authority, status, stale_after, created_at, updated_at,
            metadata_json
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13, ?14)
         ON CONFLICT(project_id, canonical_uri) DO UPDATE SET
            namespace = excluded.namespace, external_id = excluded.external_id,
            kind = excluded.kind, title = excluded.title, description = excluded.description,
            origin_kind = excluded.origin_kind, origin_id = excluded.origin_id,
            authority = excluded.authority, status = excluded.status,
            stale_after = excluded.stale_after, updated_at = excluded.updated_at,
            metadata_json = excluded.metadata_json",
        params![
            input.project_id,
            input.namespace,
            input.external_id,
            input.canonical_uri,
            input.kind,
            input.title,
            input.description,
            input.origin_kind,
            input.origin_id,
            input.authority,
            input.status.unwrap_or("stable"),
            input.stale_after,
            now,
            metadata,
        ],
    )?;
    tx.query_row(
        "SELECT id FROM resources WHERE project_id = ?1 AND canonical_uri = ?2",
        params![input.project_id, input.canonical_uri],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

/// Normalize a path to the repository-relative form stored in `files.path`.
///
/// Producers outside this crate must resolve paths through here rather than
/// building their own relative string, or they will not match what was indexed.
/// Both arguments must already be canonicalized to the same form: on Windows a
/// canonicalized path carries a `\\?\` prefix that a bare `current_dir` does
/// not, and mixing the two silently yields a path that matches nothing.
pub fn repo_relative_path(project_root: &Path, path: &Path) -> Result<String> {
    normalized_repo_path(project_root, path)
}

pub fn canonical_repo_uri(project_id: &str, project_root: &Path, path: &Path) -> Result<String> {
    if project_id.trim().is_empty() {
        bail!("project_id must be non-empty");
    }
    Ok(format!(
        "repo://{project_id}/{}",
        normalized_repo_path(project_root, path)?
    ))
}

pub fn canonical_symbol_uri(project_id: &str, repo_path: &str, stable_key: &str) -> Result<String> {
    if project_id.trim().is_empty() || stable_key.trim().is_empty() {
        bail!("project_id and stable_key must be non-empty");
    }
    let path = normalized_relative_components(Path::new(repo_path))?;
    if stable_key.contains(['\0', '#']) {
        bail!("symbol stable_key contains an unsafe delimiter");
    }
    Ok(format!("symbol://{project_id}/{path}#{stable_key}"))
}

fn normalized_repo_path(project_root: &Path, path: &Path) -> Result<String> {
    let relative = if path.is_absolute() {
        path.strip_prefix(project_root).with_context(|| {
            format!(
                "path {} is outside project root {}",
                path.display(),
                project_root.display()
            )
        })?
    } else {
        path
    };
    normalized_relative_components(relative)
}

fn normalized_relative_components(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().context("path is not valid UTF-8")?;
                if value.contains('\0') || value.is_empty() {
                    bail!("path contains an unsafe component");
                }
                parts.push(value);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("path escapes the project root")
            }
        }
    }
    if parts.is_empty() {
        bail!("path must identify a project resource");
    }
    Ok(parts.join("/"))
}

fn legacy_symbol_key(legacy_id: i64, parent: Option<&str>, name: &str, kind: &str) -> String {
    let semantic = match parent {
        Some(parent) if !parent.is_empty() => format!("{parent}::{name}:{kind}"),
        _ => format!("{name}:{kind}"),
    };
    format!("legacy-{legacy_id}-{semantic}").replace('#', "-")
}

fn row_to_resource(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResourceMatch> {
    Ok(ResourceMatch {
        id: row.get(0)?,
        canonical_uri: row.get(1)?,
        namespace: row.get(2)?,
        kind: row.get(3)?,
        title: row.get(4)?,
        authority: row.get(5)?,
        origin_kind: row.get(6)?,
        origin_id: row.get(7)?,
        status: row.get(8)?,
        current_version_id: row.get(9)?,
    })
}

fn optional_usize(value: Option<usize>) -> Result<Option<i64>> {
    value.map(i64::try_from).transpose().map_err(Into::into)
}

fn to_i64(value: usize) -> Result<i64> {
    i64::try_from(value).map_err(Into::into)
}

fn now_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn is_sqlite_lock_error(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<rusqlite::Error>()
            .is_some_and(|sqlite_err| {
                matches!(
                    sqlite_err.sqlite_error_code(),
                    Some(ErrorCode::DatabaseBusy) | Some(ErrorCode::DatabaseLocked)
                )
            })
    })
}

const SCHEMA_V1: &str = r#"
CREATE TABLE schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL
);

CREATE TABLE resources (
    id INTEGER PRIMARY KEY,
    project_id TEXT NOT NULL,
    namespace TEXT NOT NULL,
    external_id TEXT NOT NULL,
    canonical_uri TEXT NOT NULL,
    kind TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    origin_kind TEXT NOT NULL,
    origin_id TEXT NOT NULL,
    authority TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'stable',
    stale_after TEXT,
    current_version_id INTEGER REFERENCES resource_versions(id) DEFERRABLE INITIALLY DEFERRED,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE(project_id, canonical_uri)
);

CREATE TABLE resource_versions (
    id INTEGER PRIMARY KEY,
    resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    source_format TEXT NOT NULL,
    media_type TEXT,
    body TEXT,
    raw_metadata TEXT,
    content_hash TEXT NOT NULL,
    generated_by TEXT,
    generated_at TEXT,
    indexed_at INTEGER NOT NULL,
    UNIQUE(resource_id, content_hash)
);

CREATE TABLE content_segments (
    id INTEGER PRIMARY KEY,
    resource_version_id INTEGER NOT NULL REFERENCES resource_versions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    heading_path TEXT,
    text TEXT NOT NULL,
    start_line INTEGER,
    end_line INTEGER,
    start_byte INTEGER,
    end_byte INTEGER,
    token_count INTEGER,
    content_hash TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE(resource_version_id, ordinal)
);

CREATE VIRTUAL TABLE content_segments_fts USING fts5(title, heading_path, text);

CREATE TRIGGER content_segments_ai AFTER INSERT ON content_segments BEGIN
    INSERT INTO content_segments_fts(rowid, title, heading_path, text)
    VALUES (new.id, new.title, new.heading_path, new.text);
END;
CREATE TRIGGER content_segments_ad AFTER DELETE ON content_segments BEGIN
    DELETE FROM content_segments_fts WHERE rowid = old.id;
END;
CREATE TRIGGER content_segments_au AFTER UPDATE ON content_segments BEGIN
    DELETE FROM content_segments_fts WHERE rowid = old.id;
    INSERT INTO content_segments_fts(rowid, title, heading_path, text)
    VALUES (new.id, new.title, new.heading_path, new.text);
END;

CREATE TABLE edges (
    id INTEGER PRIMARY KEY,
    src_resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    dst_resource_id INTEGER REFERENCES resources(id) ON DELETE SET NULL,
    dst_ref TEXT,
    relation TEXT NOT NULL,
    confidence TEXT NOT NULL,
    extractor TEXT NOT NULL,
    source_resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    source_version_id INTEGER NOT NULL REFERENCES resource_versions(id) ON DELETE CASCADE,
    start_line INTEGER,
    end_line INTEGER,
    start_byte INTEGER,
    end_byte INTEGER,
    content_hash TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    CHECK(dst_resource_id IS NOT NULL OR dst_ref IS NOT NULL)
);

CREATE TABLE files (
    resource_id INTEGER PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    path TEXT NOT NULL UNIQUE,
    language TEXT,
    extension TEXT NOT NULL DEFAULT '',
    size INTEGER NOT NULL DEFAULT 0,
    mtime_secs INTEGER NOT NULL DEFAULT 0,
    mtime_nanos INTEGER NOT NULL DEFAULT 0,
    content_hash TEXT,
    scan_id TEXT
);

CREATE TABLE symbols (
    resource_id INTEGER PRIMARY KEY REFERENCES resources(id) ON DELETE CASCADE,
    file_resource_id INTEGER NOT NULL REFERENCES files(resource_id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    symbol_kind TEXT NOT NULL,
    parent_resource_id INTEGER REFERENCES symbols(resource_id) ON DELETE SET NULL,
    stable_key TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    start_byte INTEGER,
    end_byte INTEGER,
    language TEXT NOT NULL,
    UNIQUE(file_resource_id, stable_key)
);

CREATE TABLE tags (
    resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY(resource_id, tag)
);

CREATE TABLE provenance (
    id INTEGER PRIMARY KEY,
    resource_version_id INTEGER NOT NULL REFERENCES resource_versions(id) ON DELETE CASCADE,
    source_resource_id INTEGER REFERENCES resources(id) ON DELETE SET NULL,
    source_ref TEXT NOT NULL,
    author TEXT,
    usage_count INTEGER,
    last_modified TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE verifications (
    id INTEGER PRIMARY KEY,
    resource_version_id INTEGER NOT NULL REFERENCES resource_versions(id) ON DELETE CASCADE,
    actor_resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    verified_at TEXT,
    verification_kind TEXT NOT NULL,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE resource_aliases (
    id INTEGER PRIMARY KEY,
    project_id TEXT NOT NULL,
    resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    alias_uri TEXT NOT NULL,
    reason TEXT,
    UNIQUE(project_id, alias_uri)
);

CREATE TABLE producer_state (
    source_resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    producer TEXT NOT NULL,
    input_hash TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(source_resource_id, producer)
);

CREATE INDEX idx_resources_namespace ON resources(project_id, namespace);
CREATE INDEX idx_resources_kind ON resources(project_id, kind);
CREATE INDEX idx_resources_origin ON resources(project_id, origin_kind, origin_id);
CREATE INDEX idx_resources_status ON resources(project_id, status);
CREATE INDEX idx_versions_resource ON resource_versions(resource_id, indexed_at);
CREATE INDEX idx_segments_version ON content_segments(resource_version_id, ordinal);
CREATE INDEX idx_edges_src_relation ON edges(src_resource_id, relation);
CREATE INDEX idx_edges_dst_relation ON edges(dst_resource_id, relation);
CREATE INDEX idx_edges_owner ON edges(source_version_id, extractor);
CREATE INDEX idx_symbols_name ON symbols(name);
CREATE INDEX idx_symbols_kind ON symbols(symbol_kind);
CREATE INDEX idx_symbols_file ON symbols(file_resource_id);
CREATE INDEX idx_producer_hash ON producer_state(producer, input_hash);
CREATE UNIQUE INDEX idx_provenance_identity
    ON provenance(resource_version_id, source_ref);
CREATE UNIQUE INDEX idx_verification_identity
    ON verifications(
        resource_version_id,
        actor_resource_id,
        verification_kind,
        COALESCE(verified_at, '')
    );
"#;

/// Access accounting for the self-enhancing loop.
///
/// One row per (resource, tool) rather than one per access: the table is
/// bounded by the resource count no matter how much the tools are used, so
/// tracking can never grow without limit.
const SCHEMA_V2: &str = r#"
CREATE TABLE resource_access (
    resource_id INTEGER NOT NULL REFERENCES resources(id) ON DELETE CASCADE,
    tool TEXT NOT NULL,
    access_count INTEGER NOT NULL DEFAULT 0,
    first_access INTEGER NOT NULL,
    last_access INTEGER NOT NULL,
    PRIMARY KEY(resource_id, tool)
);

CREATE INDEX idx_resource_access_recent ON resource_access(last_access);
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn resource<'a>(metadata: &'a Value, uri: &'a str, title: &'a str) -> ResourceInput<'a> {
        ResourceInput {
            project_id: "fixture",
            namespace: "okf",
            external_id: title,
            canonical_uri: uri,
            kind: "Runbook",
            title,
            description: Some("fixture resource"),
            origin_kind: "repository",
            origin_id: "fixture-bundle",
            authority: "repository",
            status: None,
            stale_after: None,
            metadata,
        }
    }

    fn version(resource_id: i64, hash: &str) -> ResourceVersionInput<'_> {
        ResourceVersionInput {
            resource_id,
            revision: hash,
            source_format: "okf/0.2",
            media_type: Some("text/markdown"),
            body: Some("# Recovery\n\nRestart checkout."),
            raw_metadata: Some("type: Runbook"),
            content_hash: hash,
            generated_by: None,
            generated_at: None,
        }
    }

    fn segment(
        ordinal: usize,
        heading: &'static str,
        text: &'static str,
    ) -> ContentSegmentInput<'static> {
        ContentSegmentInput {
            ordinal,
            title: heading,
            heading_path: Some(heading),
            text,
            start_line: Some(1),
            end_line: Some(3),
            start_byte: Some(0),
            end_byte: Some(text.len()),
            token_count: Some(3),
            content_hash: text,
            metadata: &serde_json::Value::Null,
        }
    }

    #[test]
    fn creates_current_versioned_schema() {
        let index = ProjectIndex::open_ephemeral().unwrap();
        assert!(index.is_ephemeral());
        assert_eq!(index.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(index.count("resources").unwrap(), 0);
    }

    #[test]
    fn resources_versions_segments_and_search_are_idempotent() {
        let metadata = serde_json::json!({"unknown": {"retained": true}});
        let mut index = ProjectIndex::open_ephemeral().unwrap();
        let id = index
            .ensure_resource(&resource(
                &metadata,
                "okf://fixture/bundle/recovery",
                "Recovery",
            ))
            .unwrap();
        let same_id = index
            .ensure_resource(&resource(
                &metadata,
                "okf://fixture/bundle/recovery",
                "Recovery",
            ))
            .unwrap();
        assert_eq!(id, same_id);

        let version_id = index.put_version(&version(id, "hash-1")).unwrap();
        assert_eq!(
            version_id,
            index.put_version(&version(id, "hash-1")).unwrap()
        );
        index
            .replace_segments(
                version_id,
                &[ContentSegmentInput {
                    ordinal: 0,
                    title: "Recovery",
                    heading_path: Some("Recovery"),
                    text: "Restart checkout safely.",
                    start_line: Some(1),
                    end_line: Some(3),
                    start_byte: Some(0),
                    end_byte: Some(26),
                    token_count: Some(3),
                    content_hash: "segment-1",
                    metadata: &serde_json::json!({}),
                }],
            )
            .unwrap();

        let matches = index.search_segments("checkout", 10).unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].resource.id, id);
        assert_eq!(index.count("resource_versions").unwrap(), 1);
        assert_eq!(index.count("content_segments").unwrap(), 1);
    }

    #[test]
    fn search_ranks_authority_first_and_returns_one_row_per_resource() {
        let metadata = serde_json::json!({});
        let mut index = ProjectIndex::open_ephemeral().unwrap();

        // One authored runbook against many derived code concepts, all matching.
        let authored = index
            .ensure_resource(&resource(&metadata, "okf://fixture/runbook", "Runbook"))
            .unwrap();
        let authored_version = index.put_version(&version(authored, "authored")).unwrap();
        index
            .replace_segments(
                authored_version,
                &[
                    segment(0, "Recovery", "Restart checkout safely."),
                    segment(1, "Detail", "Checkout stalls need a restart."),
                ],
            )
            .unwrap();

        for ordinal in 0..20 {
            let uri = format!("okf://fixture/derived-{ordinal}");
            let derived = index
                .ensure_resource(&ResourceInput {
                    origin_kind: "local-derived",
                    authority: "derived",
                    ..resource(&metadata, &uri, "Derived")
                })
                .unwrap();
            let hash = format!("derived-{ordinal}");
            let derived_version = index.put_version(&version(derived, &hash)).unwrap();
            index
                .replace_segments(
                    derived_version,
                    &[segment(0, "Checkout", "checkout checkout checkout")],
                )
                .unwrap();
        }

        let matches = index
            .search_segments_filtered("fixture", "checkout", &SearchFilter::default(), 5)
            .unwrap();
        // Derived rows are far more relevant by bm25 alone; authority wins anyway.
        assert_eq!(matches[0].resource.id, authored);
        assert_eq!(matches[0].resource.authority, "repository");
        // The authored resource matched on two segments but occupies one row.
        assert_eq!(
            matches
                .iter()
                .filter(|item| item.resource.id == authored)
                .count(),
            1
        );
        let uris: Vec<_> = matches
            .iter()
            .map(|item| item.resource.canonical_uri.clone())
            .collect();
        let repeated = index
            .search_segments_filtered("fixture", "checkout", &SearchFilter::default(), 5)
            .unwrap();
        let repeated_uris: Vec<_> = repeated
            .iter()
            .map(|item| item.resource.canonical_uri.clone())
            .collect();
        assert_eq!(uris, repeated_uris, "ordering is deterministic");
    }

    #[test]
    fn access_tracking_is_bounded_accumulating_and_ordered_by_use() {
        let metadata = serde_json::json!({});
        let index = ProjectIndex::open_ephemeral().unwrap();
        let hot = index
            .ensure_resource(&resource(&metadata, "okf://fixture/hot", "Hot"))
            .unwrap();
        let cold = index
            .ensure_resource(&resource(&metadata, "okf://fixture/cold", "Cold"))
            .unwrap();

        for _ in 0..5 {
            index.record_access(hot, "read").unwrap();
        }
        index.record_access(hot, "grep").unwrap();
        index.record_access(cold, "read").unwrap();

        // One row per (resource, tool) no matter how many accesses.
        assert_eq!(index.count("resource_access").unwrap(), 3);
        assert_eq!(index.access_count(hot).unwrap(), 6);
        assert_eq!(index.access_count(cold).unwrap(), 1);

        let recent = index.recent_accesses("fixture", 0, 10).unwrap();
        assert_eq!(recent[0].0.id, hot);
        assert_eq!(recent[0].1, 6);
        assert_eq!(recent[1].0.id, cold);

        // A window that excludes everything yields nothing.
        assert!(index
            .recent_accesses("fixture", now_epoch_seconds() + 60, 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn recent_origin_pruning_caps_one_origin_without_touching_others() {
        let metadata = serde_json::json!({});
        let mut index = ProjectIndex::open_ephemeral().unwrap();
        index
            .ensure_resource(&resource(&metadata, "okf://fixture/kept", "Authored"))
            .unwrap();
        for ordinal in 0..5 {
            let uri = format!("okf://fixture/observation-{ordinal}");
            let external = format!("observations/{ordinal}.md");
            index
                .ensure_resource(&ResourceInput {
                    external_id: &external,
                    origin_id: "okf-observe",
                    origin_kind: "local-derived",
                    authority: "derived",
                    ..resource(&metadata, &uri, "Observation")
                })
                .unwrap();
        }

        let removed = index
            .prune_origin_to_recent("fixture", "okf", "okf-observe", 2)
            .unwrap();
        assert_eq!(removed, 3);
        // The authored resource lives in a different origin and is untouched.
        assert_eq!(index.count("resources").unwrap(), 3);
    }

    #[test]
    fn edge_replacement_is_scoped_to_source_version_and_extractor() {
        let metadata = serde_json::json!({});
        let mut index = ProjectIndex::open_ephemeral().unwrap();
        let source = index
            .ensure_resource(&resource(&metadata, "okf://fixture/source", "Source"))
            .unwrap();
        let target = index
            .ensure_resource(&resource(&metadata, "okf://fixture/target", "Target"))
            .unwrap();
        let version_id = index.put_version(&version(source, "source-v1")).unwrap();
        let edge = |relation, extractor| EdgeInput {
            src_resource_id: source,
            dst_resource_id: Some(target),
            dst_ref: Some("target.md"),
            relation,
            confidence: "extracted",
            extractor,
            source_resource_id: source,
            start_line: Some(3),
            end_line: Some(3),
            start_byte: None,
            end_byte: None,
            content_hash: relation,
            metadata: &metadata,
        };
        index
            .replace_edges_for_source(version_id, "markdown/1", &[edge("links_to", "markdown/1")])
            .unwrap();
        index
            .replace_edges_for_source(version_id, "manual/1", &[edge("documents", "manual/1")])
            .unwrap();
        index
            .replace_edges_for_source(version_id, "markdown/1", &[edge("cites", "markdown/1")])
            .unwrap();

        let edges = index.edges_from(source, None).unwrap();
        assert_eq!(edges.len(), 2);
        assert_eq!(
            edges
                .iter()
                .map(|edge| edge.relation.as_str())
                .collect::<Vec<_>>(),
            vec!["cites", "documents"]
        );

        let invalid = edge("links_to", "wrong-owner");
        assert!(index
            .replace_edges_for_source(version_id, "markdown/1", &[invalid])
            .is_err());
        let after_rollback = index.edges_from(source, None).unwrap();
        assert_eq!(after_rollback, edges);
    }

    #[test]
    fn tags_provenance_verification_and_aliases_are_idempotent() {
        let metadata = serde_json::json!({});
        let index = ProjectIndex::open_ephemeral().unwrap();
        let concept = index
            .ensure_resource(&resource(&metadata, "okf://fixture/concept", "Concept"))
            .unwrap();
        let actor = index
            .ensure_resource(&ResourceInput {
                project_id: "fixture",
                namespace: "actor",
                external_id: "human:reviewer",
                canonical_uri: "actor:human:reviewer",
                kind: "human",
                title: "Reviewer",
                description: None,
                origin_kind: "repository",
                origin_id: "fixture-bundle",
                authority: "repository",
                status: None,
                stale_after: None,
                metadata: &metadata,
            })
            .unwrap();
        let version_id = index.put_version(&version(concept, "concept-v1")).unwrap();

        for _ in 0..2 {
            index.add_tag(concept, "operations").unwrap();
            index
                .add_alias(
                    "fixture",
                    concept,
                    "okf://fixture/old-concept",
                    Some("moved"),
                )
                .unwrap();
            index
                .add_provenance(&ProvenanceInput {
                    resource_version_id: version_id,
                    source_resource_id: None,
                    source_ref: "repo://fixture/docs/design.md",
                    author: Some("Author"),
                    usage_count: Some(1),
                    last_modified: Some("2026-08-15"),
                    metadata: &metadata,
                })
                .unwrap();
            index
                .add_verification(&VerificationInput {
                    resource_version_id: version_id,
                    actor_resource_id: actor,
                    verified_at: Some("2026-08-15T00:00:00Z"),
                    verification_kind: "human",
                    metadata: &metadata,
                })
                .unwrap();
        }

        assert_eq!(index.count("tags").unwrap(), 1);
        assert_eq!(index.count("resource_aliases").unwrap(), 1);
        assert_eq!(index.count("provenance").unwrap(), 1);
        assert_eq!(index.count("verifications").unwrap(), 1);
    }

    #[test]
    fn migrates_legacy_file_and_symbol_indexes_without_modifying_sources() {
        let project = TempDir::new().unwrap();
        let databases = TempDir::new().unwrap();
        let files_path = databases.path().join("files.db");
        let symbols_path = databases.path().join("symbols.db");
        let files_sql =
            include_str!("../../agent-cli/tests/fixtures/knowledge_graph/migration/files-v0.sql");
        let symbols_sql =
            include_str!("../../agent-cli/tests/fixtures/knowledge_graph/migration/symbols-v0.sql");
        Connection::open(&files_path)
            .unwrap()
            .execute_batch(files_sql)
            .unwrap();
        Connection::open(&symbols_path)
            .unwrap()
            .execute_batch(symbols_sql)
            .unwrap();

        let mut index = ProjectIndex::open_ephemeral().unwrap();
        let stats = index
            .migrate_legacy_indexes(
                "fixture",
                project.path(),
                Some(&files_path),
                Some(&symbols_path),
            )
            .unwrap();
        assert_eq!(stats.files_seen, 1);
        assert_eq!(stats.files_migrated, 1);
        assert_eq!(stats.symbols_seen, 1);
        assert_eq!(stats.symbols_migrated, 1);
        assert_eq!(stats.versions_migrated, 1);
        assert_eq!(index.count("files").unwrap(), 1);
        assert_eq!(index.count("symbols").unwrap(), 1);
        let second = index
            .migrate_legacy_indexes(
                "fixture",
                project.path(),
                Some(&files_path),
                Some(&symbols_path),
            )
            .unwrap();
        assert_eq!(second, stats);
        assert_eq!(index.count("resources").unwrap(), 2);
        assert_eq!(index.count("resource_versions").unwrap(), 1);
        assert_eq!(index.count("files").unwrap(), 1);
        assert_eq!(index.count("symbols").unwrap(), 1);
        assert!(files_path.exists());
        assert!(symbols_path.exists());
    }

    #[test]
    fn rejects_newer_schema_and_unsafe_paths() {
        let databases = TempDir::new().unwrap();
        let path = databases.path().join("future.db");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", CURRENT_SCHEMA_VERSION + 1)
            .unwrap();
        drop(conn);
        let error = ProjectIndex::open(&path).err().expect("newer schema fails");
        assert!(format!("{error:#}").contains("newer than supported"));

        assert!(canonical_repo_uri("fixture", Path::new("/repo"), Path::new("../escape")).is_err());
        assert!(canonical_symbol_uri("fixture", "src/lib.rs", "bad#key").is_err());
    }

    #[test]
    fn a_version_one_index_upgrades_in_place_without_losing_data() {
        let databases = TempDir::new().unwrap();
        let path = databases.path().join("v1.db");

        // Stand up a v1 index exactly as the previous release left it.
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA_V1).unwrap();
        conn.execute(
            "INSERT INTO resources (project_id, namespace, external_id, canonical_uri, kind,
                                    title, origin_kind, origin_id, authority, status,
                                    created_at, updated_at, metadata_json)
             VALUES ('fixture', 'okf', 'kept.md', 'okf://fixture/kept', 'Runbook', 'Kept',
                     'repository', 'bundle', 'repository', 'stable', 1, 1, '{}')",
            [],
        )
        .unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        drop(conn);

        let index = ProjectIndex::open(&path).unwrap();
        assert_eq!(index.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        // Pre-existing rows survive the upgrade, and the new table is usable.
        assert_eq!(index.count("resources").unwrap(), 1);
        assert_eq!(index.count("resource_access").unwrap(), 0);
        let resource_id = index
            .resource_id_by_external_id("fixture", "okf", "kept.md")
            .unwrap()
            .expect("migrated resource");
        index.record_access(resource_id, "read").unwrap();
        assert_eq!(index.access_count(resource_id).unwrap(), 1);
    }

    #[test]
    fn graph_traversal_is_bounded_cycle_safe_and_retains_unresolved_references() {
        let metadata = serde_json::json!({});
        let mut index = ProjectIndex::open_ephemeral().unwrap();
        let a = index
            .ensure_resource(&resource(&metadata, "okf://fixture/a", "A"))
            .unwrap();
        let b = index
            .ensure_resource(&resource(&metadata, "okf://fixture/b", "B"))
            .unwrap();
        let a_version = index.put_version(&version(a, "a-v1")).unwrap();
        let b_version = index.put_version(&version(b, "b-v1")).unwrap();
        index
            .replace_edges_for_source(
                a_version,
                "fixture/1",
                &[
                    EdgeInput {
                        src_resource_id: a,
                        dst_resource_id: Some(b),
                        dst_ref: Some("B"),
                        relation: "links_to",
                        confidence: "resolved",
                        extractor: "fixture/1",
                        source_resource_id: a,
                        start_line: Some(2),
                        end_line: Some(2),
                        start_byte: None,
                        end_byte: None,
                        content_hash: "a-b",
                        metadata: &metadata,
                    },
                    EdgeInput {
                        src_resource_id: a,
                        dst_resource_id: None,
                        dst_ref: Some("missing"),
                        relation: "links_to",
                        confidence: "extracted",
                        extractor: "fixture/1",
                        source_resource_id: a,
                        start_line: Some(3),
                        end_line: Some(3),
                        start_byte: None,
                        end_byte: None,
                        content_hash: "a-missing",
                        metadata: &metadata,
                    },
                ],
            )
            .unwrap();
        index
            .replace_edges_for_source(
                b_version,
                "fixture/1",
                &[EdgeInput {
                    src_resource_id: b,
                    dst_resource_id: Some(a),
                    dst_ref: Some("A"),
                    relation: "links_to",
                    confidence: "resolved",
                    extractor: "fixture/1",
                    source_resource_id: b,
                    start_line: Some(1),
                    end_line: Some(1),
                    start_byte: None,
                    end_byte: None,
                    content_hash: "b-a",
                    metadata: &metadata,
                }],
            )
            .unwrap();

        let traversed = index.traverse(a, Some("links_to"), "both", 50, 50).unwrap();
        assert_eq!(traversed.len(), 3, "each edge appears at most once");
        assert!(traversed
            .iter()
            .any(|edge| edge.unresolved_ref.as_deref() == Some("missing")));
        assert!(traversed.iter().all(|edge| edge.depth <= 2));
        assert_eq!(index.traverse(a, None, "out", 0, 10).unwrap(), vec![]);
        assert!(index.traverse(a, None, "sideways", 1, 10).is_err());
    }
}
