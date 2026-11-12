//! Load sqlite-vec into the process before SQLite connections open.
//!
//! `sqlite3_auto_extension` applies to later `sqlite3_open` calls, so this
//! runs at the start of `Database::open`.

use std::sync::Once;

/// Register the vec0 extension with the sqlite that sqlx links.
pub fn install_sqlite_vec() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: `sqlite3_vec_init` is the sqlite-vec 0.1.9 auto-extension
        // entry point. sqlite-vec's own test transmutes it into
        // `sqlite3_auto_extension` the same way, and this runs before any
        // connection opens.
        unsafe {
            libsqlite3_sys::sqlite3_auto_extension(Some(std::mem::transmute::<
                *const (),
                unsafe extern "C" fn(
                    *mut libsqlite3_sys::sqlite3,
                    *mut *mut std::ffi::c_char,
                    *const libsqlite3_sys::sqlite3_api_routines,
                ) -> i32,
            >(sqlite_vec::sqlite3_vec_init as *const ())));
        }
    });
}
