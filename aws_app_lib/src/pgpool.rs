use deadpool_postgres::{Client, Config, Pool};
use derive_more::Deref;
use stack_string::StackString;
use std::{fmt, sync::Arc};
pub use tokio_postgres::Transaction as PgTransaction;
use tokio_postgres::{Config as PgConfig, NoTls};

use crate::errors::AwslibError as Error;

#[derive(Clone, Deref)]
pub struct PgPool {
    pgurl: Arc<StackString>,
    #[deref]
    pool: Pool,
}

impl fmt::Debug for PgPool {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PgPool {}", &self.pgurl)
    }
}

impl PgPool {
    /// # Errors
    /// Returns error if pool configuration fails
    pub fn new(pgurl: &str) -> Result<Self, Error> {
        let pgconf: PgConfig = pgurl.parse()?;

        let mut config = Config::default();

        if let tokio_postgres::config::Host::Tcp(s) = &pgconf.get_hosts()[0] {
            config.host.replace(s.to_string());
        }
        if let Some(u) = pgconf.get_user() {
            config.user.replace(u.to_string());
        }
        if let Some(p) = pgconf.get_password() {
            config
                .password
                .replace(String::from_utf8_lossy(p).to_string());
        }
        if let Some(db) = pgconf.get_dbname() {
            config.dbname.replace(db.to_string());
        }

        let pool = config.builder(NoTls)?.max_size(4).build()?;

        Ok(Self {
            pgurl: Arc::new(pgurl.into()),
            pool,
        })
    }

    /// # Errors
    /// Returns error if fail to get client from connection pool
    pub async fn get(&self) -> Result<Client, Error> {
        self.pool.get().await.map_err(Into::into)
    }
}
