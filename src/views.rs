//! In-memory view counting with batched persistence.
//!
//! Reading a document never writes to `SQLite`. Readers only bump counters in
//! memory, and a background flusher drains those increments into the
//! `document_view_stats` and `document_view_daily` tables every few seconds.
//! The counters are deliberately approximate: a crash can lose at most one
//! flush interval, which is the right trade-off for a metric whose job is to
//! show how much attention a document receives.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use crate::db::{Database, DocumentViewStats, ViewDelta};

/// How long reads accumulate in memory before they are flushed to `SQLite`.
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);

/// How many distinct readers are remembered per document and day so that
/// refreshes, prefetches, and retries do not inflate the unique count.
const MAX_UNIQUE_READERS_PER_DOCUMENT: usize = 4096;

/// `User-Agent` fragments that identify crawlers, agents, and command-line or
/// library HTTP clients. Those readers still count toward `views_total` but
/// never toward `views_human` or `views_unique_human`.
const MACHINE_USER_AGENT_MARKERS: [&str; 40] = [
    "bot",
    "spider",
    "crawl",
    "slurp",
    "curl",
    "wget",
    "httpie",
    "python",
    "httpx",
    "aiohttp",
    "requests/",
    "reqwest",
    "go-http-client",
    "java/",
    "okhttp",
    "libwww",
    "node-fetch",
    "undici",
    "axios",
    "headless",
    "phantomjs",
    "puppeteer",
    "playwright",
    "selenium",
    "openai",
    "chatgpt",
    "gptbot",
    "claudebot",
    "anthropic",
    "perplexity",
    "ccbot",
    "bytespider",
    "googlebot",
    "bingbot",
    "yandex",
    "baiduspider",
    "duckduck",
    "applebot",
    "semrush",
    "ahrefs",
];

#[derive(Debug, Default)]
struct DocumentCounters {
    total: u64,
    human: u64,
    unique_pending: u64,
    unique_seen: HashSet<u64>,
}

#[derive(Debug, Default)]
struct DayCounters {
    documents: HashMap<String, DocumentCounters>,
}

#[derive(Debug, Default)]
struct Counters {
    days: HashMap<String, DayCounters>,
}

/// One observed read of a published document.
#[derive(Clone, Copy, Debug)]
pub struct ViewEvent<'a> {
    /// The document that was read.
    pub document_id: &'a str,
    /// A best-effort client hint such as `CF-Connecting-IP`. It is hashed
    /// before it is remembered and never stored verbatim.
    pub client: Option<&'a str>,
    /// The raw `User-Agent` header, classified into human or machine readers.
    pub user_agent: Option<&'a str>,
}

/// A process-local counter that batches document reads between flushes.
pub struct ViewCounter {
    salt: u64,
    counters: Mutex<Counters>,
}

impl std::fmt::Debug for ViewCounter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ViewCounter")
            .finish_non_exhaustive()
    }
}

impl Default for ViewCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewCounter {
    /// Creates an empty counter with a fresh fingerprinting salt.
    #[must_use]
    pub fn new() -> Self {
        Self {
            salt: RandomState::new().hash_one(0_u64),
            counters: Mutex::new(Counters::default()),
        }
    }

    /// Records one successful read of `event.document_id` in memory.
    ///
    /// This never touches `SQLite`; the increment becomes durable when the
    /// flusher drains it.
    pub fn record(&self, event: ViewEvent<'_>) {
        let day = current_day();
        let fingerprint = event.client.map(|client| self.fingerprint(client));
        let is_human = !is_machine_user_agent(event.user_agent);
        let mut counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        let document = counters
            .days
            .entry(day)
            .or_default()
            .documents
            .entry(event.document_id.to_owned())
            .or_default();
        document.total += 1;
        if is_human {
            document.human += 1;
            if let Some(fingerprint) = fingerprint
                && document.unique_seen.len() < MAX_UNIQUE_READERS_PER_DOCUMENT
                && document.unique_seen.insert(fingerprint)
            {
                document.unique_pending += 1;
            }
        }
    }

    /// Removes and returns the increments accumulated since the last drain.
    ///
    /// Totals reset to zero while today's reader fingerprints stay remembered,
    /// so a reader does not become "unique" again after a flush. Days before
    /// today are dropped once drained.
    #[must_use]
    pub fn drain(&self) -> Vec<ViewDelta> {
        let today = current_day();
        let mut counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        let mut deltas = Vec::new();
        for (day, day_counters) in &mut counters.days {
            for (document_id, document) in &mut day_counters.documents {
                if document.total == 0 && document.human == 0 && document.unique_pending == 0 {
                    continue;
                }
                deltas.push(ViewDelta {
                    day: day.clone(),
                    document_id: document_id.clone(),
                    total: document.total,
                    human: document.human,
                    unique_human: document.unique_pending,
                });
                document.total = 0;
                document.human = 0;
                document.unique_pending = 0;
            }
        }
        counters.days.retain(|day, _| day == &today);
        deltas
    }

    /// Returns drained deltas to the buffer after a failed flush so the next
    /// attempt can persist them.
    pub fn restore(&self, deltas: Vec<ViewDelta>) {
        let mut counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        for delta in deltas {
            let document = counters
                .days
                .entry(delta.day)
                .or_default()
                .documents
                .entry(delta.document_id)
                .or_default();
            document.total += delta.total;
            document.human += delta.human;
            document.unique_pending += delta.unique_human;
        }
    }

    /// Adds the not-yet-flushed increments for `document_id` onto `views` so
    /// readers see a near-real-time total without waiting for a flush.
    pub fn merge_pending(&self, document_id: &str, views: &mut DocumentViewStats) {
        let counters = self.counters.lock().unwrap_or_else(PoisonError::into_inner);
        for day in counters.days.values() {
            let Some(document) = day.documents.get(document_id) else {
                continue;
            };
            views.total += document.total;
            views.human += document.human;
            views.unique_human += document.unique_pending;
        }
    }

    fn fingerprint(&self, client: &str) -> u64 {
        // FNV-1a over the salted client hint; collisions only make the unique
        // count slightly conservative.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64 ^ self.salt;
        for byte in client.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }
}

/// Persists buffered reads to `SQLite` until the process exits.
pub async fn run_flusher(database: Database, counter: Arc<ViewCounter>) {
    loop {
        tokio::time::sleep(FLUSH_INTERVAL).await;
        let deltas = counter.drain();
        if deltas.is_empty() {
            continue;
        }
        if let Err(error) = database.apply_view_deltas(&deltas) {
            let documents = deltas.len();
            eprintln!("CTX could not persist view counts for {documents} documents: {error}");
            counter.restore(deltas);
        }
    }
}

fn current_day() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

fn is_machine_user_agent(user_agent: Option<&str>) -> bool {
    let Some(user_agent) = user_agent else {
        return true;
    };
    let user_agent = user_agent.to_ascii_lowercase();
    MACHINE_USER_AGENT_MARKERS
        .iter()
        .any(|marker| user_agent.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::{ViewCounter, ViewEvent};
    use crate::db::DocumentViewStats;

    const MOZILLA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36";
    const GPT_BOT: &str = "Mozilla/5.0 AppleWebKit/537.36 (compatible; GPTBot/1.0)";

    #[test]
    fn counts_totals_human_readers_and_unique_clients_per_document_and_day() {
        let counter = ViewCounter::new();
        for (client, user_agent) in [
            (Some("203.0.113.7"), Some(MOZILLA)),
            (Some("203.0.113.7"), Some(MOZILLA)),
            (Some("198.51.100.9"), Some(MOZILLA)),
            (Some("198.51.100.9"), Some(GPT_BOT)),
            (None, Some(GPT_BOT)),
        ] {
            counter.record(ViewEvent {
                document_id: "doc-a",
                client,
                user_agent,
            });
        }

        let deltas = counter.drain();
        assert_eq!(deltas.len(), 1);
        let delta = &deltas[0];
        assert_eq!(delta.document_id, "doc-a");
        assert_eq!(delta.total, 5);
        assert_eq!(delta.human, 3);
        assert_eq!(delta.unique_human, 2);
        assert!(counter.drain().is_empty());

        // A reader who returns after a flush still counts as a read but is
        // already known as a unique reader.
        counter.record(ViewEvent {
            document_id: "doc-a",
            client: Some("203.0.113.7"),
            user_agent: Some(MOZILLA),
        });
        let deltas = counter.drain();
        assert_eq!(deltas[0].total, 1);
        assert_eq!(deltas[0].unique_human, 0);
    }

    #[test]
    fn restores_drained_deltas_after_a_failed_flush() {
        let counter = ViewCounter::new();
        counter.record(ViewEvent {
            document_id: "doc-a",
            client: Some("203.0.113.7"),
            user_agent: Some(MOZILLA),
        });
        let deltas = counter.drain();
        counter.restore(deltas);
        let deltas = counter.drain();
        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].total, 1);
        assert_eq!(deltas[0].human, 1);
        assert_eq!(deltas[0].unique_human, 1);
    }

    #[test]
    fn merges_pending_reads_onto_materialized_statistics() {
        let counter = ViewCounter::new();
        counter.record(ViewEvent {
            document_id: "doc-a",
            client: Some("203.0.113.7"),
            user_agent: Some(MOZILLA),
        });
        counter.record(ViewEvent {
            document_id: "doc-a",
            client: Some("198.51.100.9"),
            user_agent: Some(GPT_BOT),
        });
        let mut views = DocumentViewStats {
            total: 40,
            human: 30,
            unique_human: 20,
        };
        counter.merge_pending("doc-a", &mut views);
        assert_eq!(views.total, 42);
        assert_eq!(views.human, 31);
        assert_eq!(views.unique_human, 21);
        counter.merge_pending("doc-b", &mut views);
        assert_eq!(views.total, 42);
    }
}
