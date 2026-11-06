//! The `api_cache` table (doc 04 "Caching", doc 11).

use super::{now_rfc3339, Result, Storage, StorageError};
use rusqlite::{params, OptionalExtension};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

/// Cache key: `METHOD path?sorted&query`. The query is sorted by name then value and form-encoded,
/// so the same request always gives the same key.
pub fn cache_key(method: &str, path: &str, query: &[(&str, String)]) -> String {
    let mut q: Vec<(&str, &str)> = query.iter().map(|(k, v)| (*k, v.as_str())).collect();
    q.sort();
    let mut ser = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in q {
        ser.append_pair(k, v);
    }
    let q = ser.finish();
    let method = method.to_ascii_uppercase();
    if q.is_empty() {
        format!("{method} {path}")
    } else {
        format!("{method} {path}?{q}")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CacheRow {
    pub endpoint_id: String,
    /// RFC 3339, UTC.
    pub fetched_at: String,
    /// -1 = valid until the patch changes.
    pub ttl_s: i64,
    pub patch: Option<String>,
    pub http_status: u16,
    pub body: Vec<u8>,
}

impl CacheRow {
    /// Is this row still usable? `ttl_s` is the caller's current TTL for the endpoint (from DATA-001),
    /// so a changed config applies to rows already stored.
    ///
    /// `-1` means valid until the patch changes: fresh only when the row's patch equals
    /// `current_patch`. If either is unknown the row is not fresh (refetch rather than guess).
    /// Any other negative TTL is never fresh.
    pub fn is_fresh(&self, ttl_s: i64, current_patch: Option<&str>) -> bool {
        self.is_fresh_at(OffsetDateTime::now_utc(), ttl_s, current_patch)
    }

    pub fn is_fresh_at(
        &self,
        now: OffsetDateTime,
        ttl_s: i64,
        current_patch: Option<&str>,
    ) -> bool {
        if ttl_s == -1 {
            return matches!((self.patch.as_deref(), current_patch), (Some(a), Some(b)) if a == b);
        }
        if ttl_s < 0 {
            return false;
        }
        match OffsetDateTime::parse(&self.fetched_at, &Rfc3339) {
            Ok(at) => (now - at).whole_seconds() < ttl_s,
            Err(_) => false,
        }
    }
}

impl Storage {
    pub fn cache_get(&self, key: &str) -> Result<Option<CacheRow>> {
        let row = self
            .lock()
            .query_row(
                "SELECT endpoint_id, fetched_at, ttl_s, patch, http_status, body
                 FROM api_cache WHERE cache_key = ?1",
                [key],
                |r| {
                    Ok(CacheRow {
                        endpoint_id: r.get(0)?,
                        fetched_at: r.get(1)?,
                        ttl_s: r.get(2)?,
                        patch: r.get(3)?,
                        http_status: r.get(4)?,
                        body: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    pub fn cache_put(
        &self,
        key: &str,
        endpoint_id: &str,
        ttl_s: i64,
        patch: Option<&str>,
        http_status: u16,
        body: &[u8],
    ) -> Result<()> {
        self.cache_put_at(
            &now_rfc3339()?,
            key,
            endpoint_id,
            ttl_s,
            patch,
            http_status,
            body,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn cache_put_at(
        &self,
        fetched_at: &str,
        key: &str,
        endpoint_id: &str,
        ttl_s: i64,
        patch: Option<&str>,
        http_status: u16,
        body: &[u8],
    ) -> Result<()> {
        self.lock().execute(
            "INSERT INTO api_cache (cache_key, endpoint_id, fetched_at, ttl_s, patch, http_status, body)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(cache_key) DO UPDATE SET
               endpoint_id = excluded.endpoint_id, fetched_at = excluded.fetched_at,
               ttl_s = excluded.ttl_s, patch = excluded.patch,
               http_status = excluded.http_status, body = excluded.body",
            params![key, endpoint_id, fetched_at, ttl_s, patch, http_status, body],
        )?;
        Ok(())
    }

    /// Delete expired rows (run at startup, doc 11 "Retention"). Time-based rows go when their TTL has
    /// passed. Patch-based rows (`ttl_s = -1`) go when their patch differs from `current_patch`; if the
    /// patch is unknown they are kept. Returns the number of rows deleted.
    pub fn purge_expired(&self, current_patch: Option<&str>) -> Result<usize> {
        self.purge_expired_at(OffsetDateTime::now_utc(), current_patch)
    }

    fn purge_expired_at(&self, now: OffsetDateTime, current_patch: Option<&str>) -> Result<usize> {
        let now = now
            .format(&Rfc3339)
            .map_err(|e| StorageError::Time(e.to_string()))?;
        let conn = self.lock();
        let mut n = conn.execute(
            "DELETE FROM api_cache
             WHERE ttl_s >= 0 AND unixepoch(fetched_at) + ttl_s <= unixepoch(?1)",
            [&now],
        )?;
        if let Some(p) = current_patch {
            n += conn.execute(
                "DELETE FROM api_cache WHERE ttl_s = -1 AND (patch IS NULL OR patch <> ?1)",
                [p],
            )?;
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    #[test]
    fn key_sorts_query_and_encodes() {
        let a = cache_key("get", "/v1/x", &[("b", "2".into()), ("a", "1 &".into())]);
        let b = cache_key("GET", "/v1/x", &[("a", "1 &".into()), ("b", "2".into())]);
        assert_eq!(a, b);
        assert_eq!(a, "GET /v1/x?a=1+%26&b=2");
        assert_eq!(cache_key("GET", "/v1/x", &[]), "GET /v1/x");
    }

    #[test]
    fn put_get_roundtrip_and_overwrite() {
        let s = Storage::open_in_memory().unwrap();
        assert!(s.cache_get("k").unwrap().is_none());
        s.cache_put("k", "API-01", 60, Some("p1"), 200, b"one")
            .unwrap();
        s.cache_put("k", "API-01", 60, Some("p2"), 404, b"two")
            .unwrap();
        let r = s.cache_get("k").unwrap().unwrap();
        assert_eq!(r.endpoint_id, "API-01");
        assert_eq!(r.patch.as_deref(), Some("p2"));
        assert_eq!(r.http_status, 404);
        assert_eq!(r.body, b"two");
    }

    fn row(fetched_at: OffsetDateTime, ttl_s: i64, patch: Option<&str>) -> CacheRow {
        CacheRow {
            endpoint_id: "API-01".into(),
            fetched_at: fetched_at.format(&Rfc3339).unwrap(),
            ttl_s,
            patch: patch.map(String::from),
            http_status: 200,
            body: vec![],
        }
    }

    #[test]
    fn freshness_by_time() {
        let now = OffsetDateTime::now_utc();
        let r = row(now - Duration::seconds(100), 60, None);
        assert!(r.is_fresh_at(now, 200, None));
        assert!(!r.is_fresh_at(now, 100, None));
        assert!(!r.is_fresh_at(now, 0, None));
        assert!(!r.is_fresh_at(now, -5, None));
    }

    #[test]
    fn freshness_until_patch_change() {
        let now = OffsetDateTime::now_utc();
        let r = row(now - Duration::days(400), -1, Some("1.2"));
        assert!(r.is_fresh_at(now, -1, Some("1.2")));
        assert!(!r.is_fresh_at(now, -1, Some("1.3")));
        assert!(!r.is_fresh_at(now, -1, None));
        assert!(!row(now, -1, None).is_fresh_at(now, -1, Some("1.2")));
    }

    #[test]
    fn purge_removes_only_expired() {
        let s = Storage::open_in_memory().unwrap();
        let t0 = OffsetDateTime::now_utc();
        let at = |secs: i64| (t0 - Duration::seconds(secs)).format(&Rfc3339).unwrap();
        s.cache_put_at(&at(100), "old", "API-01", 60, None, 200, b"")
            .unwrap();
        s.cache_put_at(&at(10), "young", "API-01", 60, None, 200, b"")
            .unwrap();
        s.cache_put_at(&at(10), "patch_old", "API-15", -1, Some("1.1"), 200, b"")
            .unwrap();
        s.cache_put_at(&at(10), "patch_cur", "API-15", -1, Some("1.2"), 200, b"")
            .unwrap();
        // Unknown patch: patch rows stay.
        assert_eq!(s.purge_expired_at(t0, None).unwrap(), 1);
        assert!(s.cache_get("old").unwrap().is_none());
        assert!(s.cache_get("patch_old").unwrap().is_some());
        // Known patch: the stale-patch row goes.
        assert_eq!(s.purge_expired_at(t0, Some("1.2")).unwrap(), 1);
        assert!(s.cache_get("patch_old").unwrap().is_none());
        assert!(s.cache_get("patch_cur").unwrap().is_some());
        assert!(s.cache_get("young").unwrap().is_some());
    }
}
