//! Fixed-size reader connection pool: a minimal hand-rolled checkout pool
//! (`Mutex<Vec<Connection>>` + `Condvar`) so list/stats/TOC reads never
//! queue behind the single writer. Callers already run on `spawn_blocking`
//! threads, which bounds how many can wait on a checkout at once.

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
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
    ///
    /// The connection returns to the pool on every exit path, unwinding
    /// included. That matters because a panicking read does not end the
    /// process: `inkuna-ffi` ships with unwinding panics precisely so
    /// UniFFI can convert one into an error at the boundary and leave the
    /// host app running. Dropping the connection instead would shrink the
    /// pool by one per panic and, after `READER_POOL_SIZE` of them, park
    /// every later reader on `available` forever with nothing left to wake
    /// it. A caught panic is resumed unchanged once the connection is back,
    /// so callers still observe the panic they would have seen.
    pub(crate) fn with<T>(
        &self,
        work: impl FnOnce(&Connection) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let conn = self.checkout();
        // `AssertUnwindSafe`: the only state crossing the boundary is the
        // connection, and a read that panicked leaves it with nothing but
        // finalized statements — SELECT-only work opens no transaction.
        let result = catch_unwind(AssertUnwindSafe(|| work(&conn)));
        self.release(conn);
        match result {
            Ok(result) => result,
            Err(panic) => resume_unwind(panic),
        }
    }

    /// Blocks until a connection is free, then takes it out of the pool.
    fn checkout(&self) -> Connection {
        let mut pool = self.lock_connections();
        loop {
            match pool.pop() {
                Some(conn) => return conn,
                None => {
                    pool = self
                        .available
                        .wait(pool)
                        .unwrap_or_else(PoisonError::into_inner)
                }
            }
        }
    }

    /// Puts a connection back and wakes one waiting checkout.
    fn release(&self, conn: Connection) {
        self.lock_connections().push(conn);
        self.available.notify_one();
    }

    /// Locks the pool, keeping it usable after a poisoning panic: the
    /// guarded value is a plain `Vec<Connection>` with no invariant a panic
    /// could leave broken, so recovering the guard beats propagating — and
    /// the crate bans `unwrap` outside tests either way.
    fn lock_connections(&self) -> MutexGuard<'_, Vec<Connection>> {
        self.connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
