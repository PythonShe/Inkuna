//! Fixed-size reader connection pool: a minimal hand-rolled checkout pool
//! (`Mutex<Vec<Connection>>` + `Condvar`) so list/stats/TOC reads never
//! queue behind the single writer. Callers already run on `spawn_blocking`
//! threads, which bounds how many can wait on a checkout at once.

use std::path::Path;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use rusqlite::Connection;

use crate::CoreError;

/// Pool size is a starting point, not measured; revisit with profiling.
pub(crate) const READER_POOL_SIZE: usize = 4;

pub(crate) struct ReaderPool {
    connections: Mutex<Vec<Connection>>,
    available: Condvar,
}

impl ReaderPool {
    pub(crate) fn open(db_path: &Path, size: usize) -> Result<ReaderPool, CoreError> {
        let mut connections = Vec::with_capacity(size);
        for _ in 0..size {
            connections.push(open_connection(db_path)?);
        }
        Ok(ReaderPool {
            connections: Mutex::new(connections),
            available: Condvar::new(),
        })
    }

    /// Runs `work` on a pooled connection, blocking until one is free.
    /// A panic inside `work` leaks that connection (the pool shrinks by
    /// one); panics are programming errors here, not a recovery path.
    pub(crate) fn with<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let conn = {
            let mut pool = self.connections.lock().unwrap();
            loop {
                match pool.pop() {
                    Some(conn) => break conn,
                    None => pool = self.available.wait(pool).unwrap(),
                }
            }
        };
        let result = work(&conn);
        self.connections.lock().unwrap().push(conn);
        self.available.notify_one();
        result
    }
}

/// Shared connection setup for the writer and every pooled reader.
pub(crate) fn open_connection(db_path: &Path) -> Result<Connection, CoreError> {
    let conn = Connection::open(db_path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(Duration::from_secs(5))?;
    Ok(conn)
}
