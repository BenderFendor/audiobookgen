use crate::model::{
    BookDetail, BookSummary, Chapter, Fragment, GeneratedSegment, NarrationProfile, ProgressState,
    PronunciationRule, WordTiming,
};
use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct LibraryDatabase {
    connection: Arc<Mutex<Connection>>,
}

impl LibraryDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "synchronous", "NORMAL")?;
        connection.busy_timeout(std::time::Duration::from_secs(5))?;
        let database = Self {
            connection: Arc::new(Mutex::new(connection)),
        };
        database.migrate()?;
        Ok(database)
    }

    fn migrate(&self) -> Result<()> {
        let connection = self.lock()?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS books (
              id TEXT PRIMARY KEY,
              source_sha256 TEXT NOT NULL UNIQUE,
              title TEXT NOT NULL,
              authors_json TEXT NOT NULL,
              language TEXT,
              cover_path TEXT,
              source_path TEXT NOT NULL,
              layout TEXT NOT NULL,
              active_profile_id TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chapters (
              id TEXT PRIMARY KEY,
              book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
              chapter_index INTEGER NOT NULL,
              title TEXT NOT NULL,
              href TEXT NOT NULL,
              media_type TEXT NOT NULL,
              layout TEXT NOT NULL,
              selected INTEGER NOT NULL,
              fragment_count INTEGER NOT NULL DEFAULT 0,
              UNIQUE(book_id, chapter_index)
            );
            CREATE TABLE IF NOT EXISTS profiles (
              id TEXT PRIMARY KEY,
              book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
              name TEXT NOT NULL,
              voice TEXT NOT NULL,
              speed REAL NOT NULL,
              model_revision TEXT NOT NULL,
              model_sha256 TEXT,
              normalization_version TEXT NOT NULL,
              planner_version TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS fragments (
              id TEXT PRIMARY KEY,
              book_id TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
              chapter_id TEXT NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
              chapter_index INTEGER NOT NULL,
              fragment_index INTEGER NOT NULL,
              payload_json TEXT NOT NULL,
              cache_key TEXT NOT NULL,
              UNIQUE(chapter_id, fragment_index)
            );
            CREATE INDEX IF NOT EXISTS fragments_book_index ON fragments(book_id, chapter_index, fragment_index);
            CREATE TABLE IF NOT EXISTS generated_segments (
              fragment_id TEXT NOT NULL REFERENCES fragments(id) ON DELETE CASCADE,
              profile_id TEXT NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,
              cache_key TEXT NOT NULL,
              audio_path TEXT NOT NULL,
              duration_ms INTEGER NOT NULL,
              sample_rate INTEGER NOT NULL,
              created_at TEXT NOT NULL,
              PRIMARY KEY(fragment_id, profile_id)
            );
            CREATE TABLE IF NOT EXISTS progress (
              book_id TEXT PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
              payload_json TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS pronunciation_rules (
              id TEXT PRIMARY KEY,
              book_id TEXT REFERENCES books(id) ON DELETE CASCADE,
              pattern TEXT NOT NULL,
              replacement TEXT NOT NULL,
              case_sensitive INTEGER NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL
            );
            "#,
        )?;
        add_column_if_missing(
            &connection,
            "profiles",
            "engine",
            "TEXT NOT NULL DEFAULT 'kokoro'",
        )?;
        add_column_if_missing(
            &connection,
            "generated_segments",
            "word_timings_json",
            "TEXT",
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let connection = self.lock()?;
        Ok(connection
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO settings (key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn insert_book(
        &self,
        summary: &BookSummary,
        source_sha256: &str,
        chapters: &[Chapter],
        profile: &NarrationProfile,
        fragments: &[Fragment],
    ) -> Result<()> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO books (id, source_sha256, title, authors_json, language, cover_path, source_path, layout, active_profile_id, created_at, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![summary.id.to_string(), source_sha256, summary.title, serde_json::to_string(&summary.authors)?, summary.language,
                path_string(summary.cover_path.as_deref()), path_string(Some(&summary.source_path)), serde_json::to_string(&summary.layout)?,
                summary.active_profile_id.map(|id| id.to_string()), summary.created_at.to_rfc3339(), summary.updated_at.to_rfc3339()],
        ).context("inserting book")?;
        for chapter in chapters {
            transaction.execute(
                "INSERT INTO chapters (id,book_id,chapter_index,title,href,media_type,layout,selected,fragment_count) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![chapter.id.to_string(), chapter.book_id.to_string(), chapter.index as i64, chapter.title, chapter.href, chapter.media_type,
                    serde_json::to_string(&chapter.layout)?, chapter.selected as i64, chapter.fragment_count as i64],
            )?;
        }
        insert_profile_row(&transaction, profile)?;
        for fragment in fragments {
            transaction.execute(
                "INSERT INTO fragments (id,book_id,chapter_id,chapter_index,fragment_index,payload_json,cache_key) VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![fragment.id.to_string(), fragment.book_id.to_string(), fragment.chapter_id.to_string(), fragment.chapter_index as i64,
                    fragment.index as i64, serde_json::to_string(fragment)?, fragment.cache_key],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn find_book_by_source_sha(&self, source_sha256: &str) -> Result<Option<BookDetail>> {
        let connection = self.lock()?;
        let id = connection
            .query_row(
                "SELECT id FROM books WHERE source_sha256=?1",
                [source_sha256],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        drop(connection);
        match id {
            Some(value) => self.get_book(Uuid::parse_str(&value)?),
            None => Ok(None),
        }
    }

    pub fn list_books(&self) -> Result<Vec<BookSummary>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT id FROM books ORDER BY updated_at DESC")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.into_iter()
            .map(|id| {
                get_summary(&connection, &id)?
                    .ok_or_else(|| anyhow!("book disappeared while listing"))
            })
            .collect()
    }

    pub fn get_book(&self, book_id: Uuid) -> Result<Option<BookDetail>> {
        let connection = self.lock()?;
        let Some(summary) = get_summary(&connection, &book_id.to_string())? else {
            return Ok(None);
        };
        let chapters = query_chapters(&connection, book_id)?;
        let profiles = query_profiles(&connection, book_id)?;
        Ok(Some(BookDetail {
            summary,
            chapters,
            profiles,
        }))
    }

    pub fn insert_profile(&self, profile: &NarrationProfile) -> Result<()> {
        let connection = self.lock()?;
        insert_profile_row(&connection, profile)?;
        connection.execute(
            "UPDATE books SET active_profile_id=?1, updated_at=?2 WHERE id=?3",
            params![
                profile.id.to_string(),
                Utc::now().to_rfc3339(),
                profile.book_id.to_string()
            ],
        )?;
        Ok(())
    }

    pub fn get_profile(&self, book_id: Uuid, profile_id: Uuid) -> Result<Option<NarrationProfile>> {
        let connection = self.lock()?;
        Ok(query_profiles(&connection, book_id)?
            .into_iter()
            .find(|profile| profile.id == profile_id))
    }

    pub fn set_active_profile(&self, book_id: Uuid, profile_id: Uuid) -> Result<()> {
        let connection = self.lock()?;
        let changed = connection.execute(
            "UPDATE books SET active_profile_id=?1, updated_at=?2 WHERE id=?3 AND EXISTS (SELECT 1 FROM profiles WHERE id=?1 AND book_id=?3)",
            params![profile_id.to_string(), Utc::now().to_rfc3339(), book_id.to_string()],
        )?;
        if changed == 0 {
            return Err(anyhow!("narration profile does not belong to this book"));
        }
        Ok(())
    }

    pub fn fragments_for_chapter(&self, chapter_id: Uuid) -> Result<Vec<Fragment>> {
        let connection = self.lock()?;
        query_fragments(
            &connection,
            "SELECT payload_json FROM fragments WHERE chapter_id=?1 ORDER BY fragment_index",
            chapter_id.to_string(),
        )
    }

    pub fn fragments_for_book(&self, book_id: Uuid) -> Result<Vec<Fragment>> {
        let connection = self.lock()?;
        query_fragments(
            &connection,
            "SELECT payload_json FROM fragments WHERE book_id=?1 ORDER BY chapter_index, fragment_index",
            book_id.to_string(),
        )
    }

    pub fn generated_segment(
        &self,
        fragment_id: Uuid,
        profile_id: Uuid,
    ) -> Result<Option<GeneratedSegment>> {
        let connection = self.lock()?;
        let row = connection.query_row(
            "SELECT cache_key,audio_path,duration_ms,sample_rate,word_timings_json,created_at FROM generated_segments WHERE fragment_id=?1 AND profile_id=?2",
            params![fragment_id.to_string(), profile_id.to_string()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, String>(5)?)),
        ).optional()?;
        row.map(
            |(cache_key, audio_path, duration_ms, sample_rate, word_timings, created_at)| {
                Ok(GeneratedSegment {
                    fragment_id,
                    profile_id,
                    cache_key,
                    audio_path: PathBuf::from(audio_path),
                    duration_ms: duration_ms as u64,
                    sample_rate: sample_rate as u32,
                    word_timings: parse_word_timings(word_timings.as_deref()),
                    created_at: parse_datetime(&created_at)?,
                })
            },
        )
        .transpose()
    }

    pub fn save_generated_segment(&self, segment: &GeneratedSegment) -> Result<()> {
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO generated_segments (fragment_id,profile_id,cache_key,audio_path,duration_ms,sample_rate,word_timings_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(fragment_id,profile_id) DO UPDATE SET cache_key=excluded.cache_key,audio_path=excluded.audio_path,duration_ms=excluded.duration_ms,sample_rate=excluded.sample_rate,word_timings_json=excluded.word_timings_json,created_at=excluded.created_at",
            params![segment.fragment_id.to_string(), segment.profile_id.to_string(), segment.cache_key, segment.audio_path.to_string_lossy(), segment.duration_ms as i64, segment.sample_rate as i64,
                if segment.word_timings.is_empty() { None } else { Some(serde_json::to_string(&segment.word_timings)?) },
                segment.created_at.to_rfc3339()],
        )?;
        connection.execute(
            "UPDATE books SET updated_at=?1 WHERE id=(SELECT book_id FROM fragments WHERE id=?2)",
            params![Utc::now().to_rfc3339(), segment.fragment_id.to_string()],
        )?;
        Ok(())
    }

    pub fn save_progress(&self, progress: &ProgressState) -> Result<()> {
        let connection = self.lock()?;
        let mut stored = progress.clone();
        stored.updated_at = Some(Utc::now());
        connection.execute(
            "INSERT INTO progress (book_id,payload_json,updated_at) VALUES (?1,?2,?3) ON CONFLICT(book_id) DO UPDATE SET payload_json=excluded.payload_json,updated_at=excluded.updated_at",
            params![stored.book_id.to_string(), serde_json::to_string(&stored)?, stored.updated_at.expect("set above").to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn load_progress(&self, book_id: Uuid) -> Result<Option<ProgressState>> {
        let connection = self.lock()?;
        let payload = connection
            .query_row(
                "SELECT payload_json FROM progress WHERE book_id=?1",
                [book_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        payload
            .map(|value| serde_json::from_str(&value).context("parsing progress state"))
            .transpose()
    }

    pub fn save_pronunciation_rule(&self, rule: &PronunciationRule) -> Result<()> {
        let connection = self.lock()?;
        connection.execute(
            "INSERT INTO pronunciation_rules (id,book_id,pattern,replacement,case_sensitive,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
            params![rule.id.to_string(), rule.book_id.map(|id| id.to_string()), &rule.pattern, &rule.replacement, rule.case_sensitive as i64, rule.created_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn pronunciation_rules(&self, book_id: Uuid) -> Result<Vec<PronunciationRule>> {
        let connection = self.lock()?;
        let mut statement = connection.prepare("SELECT id,book_id,pattern,replacement,case_sensitive,created_at FROM pronunciation_rules WHERE book_id IS NULL OR book_id=?1 ORDER BY book_id IS NULL DESC, created_at")?;
        let raw = statement
            .query_map([book_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.into_iter()
            .map(
                |(id, scoped_book, pattern, replacement, case_sensitive, created_at)| {
                    Ok(PronunciationRule {
                        id: Uuid::parse_str(&id)?,
                        book_id: scoped_book
                            .map(|value| Uuid::parse_str(&value))
                            .transpose()?,
                        pattern,
                        replacement,
                        case_sensitive: case_sensitive != 0,
                        created_at: parse_datetime(&created_at)?,
                    })
                },
            )
            .collect()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.connection
            .lock()
            .map_err(|_| anyhow!("database mutex poisoned"))
    }
}

fn get_summary(connection: &Connection, id: &str) -> Result<Option<BookSummary>> {
    let raw = connection.query_row(
        "SELECT id,title,authors_json,language,cover_path,source_path,layout,active_profile_id,created_at,updated_at,
         (SELECT COUNT(*) FROM chapters WHERE book_id=books.id),
         (SELECT COUNT(*) FROM fragments WHERE book_id=books.id),
         (SELECT COUNT(*) FROM generated_segments gs JOIN fragments f ON f.id=gs.fragment_id WHERE f.book_id=books.id AND gs.profile_id=books.active_profile_id)
         FROM books WHERE id=?1",
        [id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, Option<String>>(3)?, row.get::<_, Option<String>>(4)?, row.get::<_, String>(5)?, row.get::<_, String>(6)?, row.get::<_, Option<String>>(7)?, row.get::<_, String>(8)?, row.get::<_, String>(9)?, row.get::<_, i64>(10)?, row.get::<_, i64>(11)?, row.get::<_, i64>(12)?)),
    ).optional()?;
    raw.map(
        |(
            id,
            title,
            authors,
            language,
            cover,
            source,
            layout,
            active,
            created,
            updated,
            chapters,
            total,
            generated,
        )| {
            Ok(BookSummary {
                id: Uuid::parse_str(&id)?,
                title,
                authors: serde_json::from_str(&authors)?,
                language,
                cover_path: cover.map(PathBuf::from),
                source_path: PathBuf::from(source),
                layout: serde_json::from_str(&layout)?,
                chapter_count: chapters as usize,
                generated_sentences: generated as usize,
                total_sentences: total as usize,
                active_profile_id: active.map(|value| Uuid::parse_str(&value)).transpose()?,
                created_at: parse_datetime(&created)?,
                updated_at: parse_datetime(&updated)?,
            })
        },
    )
    .transpose()
}

fn query_chapters(connection: &Connection, book_id: Uuid) -> Result<Vec<Chapter>> {
    let mut statement = connection.prepare("SELECT id,chapter_index,title,href,media_type,layout,selected,fragment_count FROM chapters WHERE book_id=?1 ORDER BY chapter_index")?;
    let raw = statement
        .query_map([book_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raw.into_iter()
        .map(
            |(id, index, title, href, media_type, layout, selected, count)| {
                Ok(Chapter {
                    id: Uuid::parse_str(&id)?,
                    book_id,
                    index: index as usize,
                    title,
                    href,
                    media_type,
                    layout: serde_json::from_str(&layout)?,
                    selected: selected != 0,
                    fragment_count: count as usize,
                })
            },
        )
        .collect()
}

fn query_profiles(connection: &Connection, book_id: Uuid) -> Result<Vec<NarrationProfile>> {
    let mut statement = connection.prepare("SELECT id,name,engine,voice,speed,model_revision,model_sha256,normalization_version,planner_version,created_at FROM profiles WHERE book_id=?1 ORDER BY created_at")?;
    let raw = statement
        .query_map([book_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    raw.into_iter()
        .map(
            |(id, name, engine, voice, speed, revision, sha, normalization, planner, created)| {
                Ok(NarrationProfile {
                    id: Uuid::parse_str(&id)?,
                    book_id,
                    name,
                    engine,
                    voice,
                    speed: speed as f32,
                    model_revision: revision,
                    model_sha256: sha,
                    normalization_version: normalization,
                    planner_version: planner,
                    created_at: parse_datetime(&created)?,
                })
            },
        )
        .collect()
}

fn query_fragments(connection: &Connection, sql: &str, value: String) -> Result<Vec<Fragment>> {
    let mut statement = connection.prepare(sql)?;
    let payloads = statement
        .query_map([value], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    payloads
        .into_iter()
        .map(|payload| serde_json::from_str(&payload).context("parsing fragment"))
        .collect()
}

fn insert_profile_row(connection: &Connection, profile: &NarrationProfile) -> Result<()> {
    connection.execute(
        "INSERT INTO profiles (id,book_id,name,engine,voice,speed,model_revision,model_sha256,normalization_version,planner_version,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![profile.id.to_string(), profile.book_id.to_string(), profile.name, profile.engine, profile.voice, profile.speed as f64, profile.model_revision, profile.model_sha256, profile.normalization_version, profile.planner_version, profile.created_at.to_rfc3339()],
    )?;
    Ok(())
}

fn add_column_if_missing(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let exists: bool = connection.query_row(
        "SELECT COUNT(*) > 0 FROM pragma_table_info(?1) WHERE name=?2",
        params![table, column],
        |row| row.get(0),
    )?;
    if !exists {
        connection.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

fn parse_word_timings(payload: Option<&str>) -> Vec<WordTiming> {
    payload
        .and_then(|value| serde_json::from_str(value).ok())
        .unwrap_or_default()
}

fn parse_datetime(value: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}
fn path_string(path: Option<&Path>) -> Option<String> {
    path.map(|value| value.to_string_lossy().into_owned())
}
