use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use libsqlite3_sys::{sqlite3, sqlite3_interrupt};
use sqlx::Sqlite;
use sqlx::pool::PoolConnection;
use tablepro_core::DriverError;

/// A pointer to the open SQLite database, used only to call
/// `sqlite3_interrupt`. SQLite documents that call as safe from a thread
/// other than the one running the statement, which is what makes a Stop
/// possible at all: sqlx runs each connection's statements on its own
/// worker thread, so while a statement runs nothing else can borrow the
/// connection.
///
/// The handle never leaves the operation that made it, and that
/// operation holds the `PoolConnection` throughout, so the database
/// cannot be closed while a handle to it exists.
pub(crate) struct InterruptHandle {
    database: NonNull<sqlite3>,
    state: Arc<AtomicU8>,
}

const ARMED: u8 = 0;
const REQUESTED: u8 = 1;
const RETIRED: u8 = 2;
const PROGRESS_INTERVAL: i32 = 1000;

unsafe impl Send for InterruptHandle {}
unsafe impl Sync for InterruptHandle {}

impl InterruptHandle {
    pub(crate) async fn of(connection: &mut PoolConnection<Sqlite>) -> Result<Self, DriverError> {
        let mut handle = connection
            .lock_handle()
            .await
            .map_err(|error| DriverError::Internal(format!("sqlite connection is unusable: {error}")))?;
        let state = Arc::new(AtomicU8::new(ARMED));
        let observed = Arc::clone(&state);
        handle.set_progress_handler(PROGRESS_INTERVAL, move || observed.load(Ordering::Acquire) != REQUESTED);
        Ok(Self {
            database: handle.as_raw_handle(),
            state,
        })
    }

    /// Asks SQLite to abort the statement running on this database. The
    /// statement fails with `SQLITE_INTERRUPT`. A statement waiting on a
    /// database lock is aborted too, which a progress handler cannot do.
    pub(crate) fn interrupt(&self) {
        let _ = self
            .state
            .compare_exchange(ARMED, REQUESTED, Ordering::AcqRel, Ordering::Acquire);
        unsafe {
            sqlite3_interrupt(self.database.as_ptr());
        }
    }
}

impl Drop for InterruptHandle {
    fn drop(&mut self) {
        self.state.store(RETIRED, Ordering::Release);
    }
}
