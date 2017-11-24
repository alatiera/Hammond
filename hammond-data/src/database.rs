use r2d2_diesel::ConnectionManager;
use diesel::prelude::*;
use r2d2;

use std::path::PathBuf;
use std::sync::Arc;
use std::io;
use std::time::Duration;

use errors::*;

#[cfg(not(test))]
use xdg_;

type Pool = Arc<r2d2::Pool<ConnectionManager<SqliteConnection>>>;

embed_migrations!("migrations/");

lazy_static!{
    static ref POOL: Pool = init_pool(DB_PATH.to_str().unwrap());
}

#[cfg(not(test))]
lazy_static! {
    static ref DB_PATH: PathBuf = xdg_::HAMMOND_XDG.place_data_file("hammond.db").unwrap();
}

#[cfg(test)]
extern crate tempdir;

#[cfg(test)]
lazy_static! {
    static ref TEMPDIR: tempdir::TempDir = {
        tempdir::TempDir::new("hammond_unit_test").unwrap()
    };

    static ref DB_PATH: PathBuf = TEMPDIR.path().join("hammond.db");
}

pub fn connection() -> Pool {
    // Arc::clone(&DB)
    Arc::clone(&POOL)
}

fn init_pool(db_path: &str) -> Pool {
    let config = r2d2::Config::builder()
        .pool_size(1)
        .connection_timeout(Duration::from_secs(60))
        .build();
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);
    let pool = Arc::new(r2d2::Pool::new(config, manager).expect("Failed to create pool."));
    info!("Database pool initialized.");

    {
        let db = Arc::clone(&pool).get().unwrap();
        run_migration_on(&*db).unwrap();
    }

    pool
}

pub fn run_migration_on(connection: &SqliteConnection) -> Result<()> {
    info!("Running DB Migrations...");
    // embedded_migrations::run(connection)?;
    embedded_migrations::run_with_output(connection, &mut io::stdout())?;
    Ok(())
}