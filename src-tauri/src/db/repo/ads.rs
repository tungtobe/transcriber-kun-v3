//! Local-only house-ad display and action counters. No function in this
//! module performs network I/O or emits telemetry.

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension};

pub const IMPRESSION_CAP_SECONDS: i64 = 10 * 60;

/// Read the most recent visible impression for every creative so selection
/// can exclude capped creatives before applying weights.
pub fn last_displays(conn: &Connection) -> rusqlite::Result<HashMap<String, i64>> {
    let mut statement = conn.prepare(
        "SELECT creative_id, last_displayed_at FROM ad_creative_state \
         WHERE last_displayed_at IS NOT NULL",
    )?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    rows.collect()
}

/// Record one impression only when the prior visible impression is outside
/// the ten-minute cap. The transaction serializes concurrent display acks.
pub fn record_impression(
    conn: &mut Connection,
    creative_id: &str,
    now_unix_seconds: i64,
) -> rusqlite::Result<bool> {
    let transaction = conn.transaction()?;
    let last: Option<i64> = transaction
        .query_row(
            "SELECT last_displayed_at FROM ad_creative_state WHERE creative_id = ?1",
            params![creative_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();

    if last.is_some_and(|last| now_unix_seconds.saturating_sub(last) < IMPRESSION_CAP_SECONDS) {
        transaction.commit()?;
        return Ok(false);
    }

    transaction.execute(
        "INSERT INTO ad_creative_state \
         (creative_id, last_displayed_at, impressions, clicks, reports) \
         VALUES (?1, ?2, 1, 0, 0) \
         ON CONFLICT(creative_id) DO UPDATE SET \
             last_displayed_at = excluded.last_displayed_at, \
             impressions = ad_creative_state.impressions + 1",
        params![creative_id, now_unix_seconds],
    )?;
    transaction.commit()?;
    Ok(true)
}

/// Count a successful user-initiated external click locally.
pub fn record_click(conn: &Connection, creative_id: &str) -> rusqlite::Result<()> {
    increment_action(conn, creative_id, "clicks")
}

/// Count a successful user-initiated report action locally.
pub fn record_report(conn: &Connection, creative_id: &str) -> rusqlite::Result<()> {
    increment_action(conn, creative_id, "reports")
}

fn increment_action(
    conn: &Connection,
    creative_id: &str,
    column: &'static str,
) -> rusqlite::Result<()> {
    // `column` is selected only by the two functions above; it is never
    // derived from creative data or any IPC input.
    let sql = format!(
        "INSERT INTO ad_creative_state (creative_id, {column}) VALUES (?1, 1) \
         ON CONFLICT(creative_id) DO UPDATE SET {column} = {column} + 1"
    );
    conn.execute(&sql, params![creative_id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use tempfile::tempdir;

    fn open_migrated() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        migrations::run(&mut conn).unwrap();
        conn
    }

    #[test]
    fn visible_impression_is_persisted_and_duplicates_within_ten_minutes_are_ignored() {
        let mut conn = open_migrated();
        assert!(record_impression(&mut conn, "creative-id", 1_800_000_000).unwrap());
        assert!(!record_impression(&mut conn, "creative-id", 1_800_000_599).unwrap());
        assert!(record_impression(&mut conn, "creative-id", 1_800_000_600).unwrap());

        let state: (Option<i64>, i64) = conn
            .query_row(
                "SELECT last_displayed_at, impressions FROM ad_creative_state \
                 WHERE creative_id = 'creative-id'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, (Some(1_800_000_600), 2));
    }

    #[test]
    fn impression_cap_survives_database_reopen() {
        let dir = tempdir().unwrap();
        {
            let mut conn = Connection::open(dir.path().join("app.db")).unwrap();
            migrations::run(&mut conn).unwrap();
            assert!(record_impression(&mut conn, "persistent-id", 1_800_000_000).unwrap());
        }
        let mut reopened = Connection::open(dir.path().join("app.db")).unwrap();
        migrations::run(&mut reopened).unwrap();
        assert!(!record_impression(&mut reopened, "persistent-id", 1_800_000_300).unwrap());
        assert_eq!(
            last_displays(&reopened).unwrap()["persistent-id"],
            1_800_000_000
        );
    }

    #[test]
    fn local_click_and_report_counts_never_require_a_network_operation() {
        let conn = open_migrated();
        record_click(&conn, "local-id").unwrap();
        record_report(&conn, "local-id").unwrap();
        let counts: (i64, i64) = conn
            .query_row(
                "SELECT clicks, reports FROM ad_creative_state WHERE creative_id = 'local-id'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 1));
    }
}
