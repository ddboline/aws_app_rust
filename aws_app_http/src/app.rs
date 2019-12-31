use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{web, App, HttpServer};
use chrono::Duration;
use std::time;
use tokio::time::interval;

use aws_app_lib::config::Config;
use aws_app_lib::pgpool::PgPool;

use super::logged_user::AUTHORIZED_USERS;
use super::routes::sync_frontpage;

pub struct AppState {
    pub db: PgPool,
}

pub async fn start_app() {
    let config = Config::init_config().expect("Failed to load config");
    let pool = PgPool::new(&config.database_url);

    async fn _update_db(pool: PgPool) {
        let mut i = interval(time::Duration::from_secs(60));
        loop {
            i.tick().await;
            AUTHORIZED_USERS.fill_from_db(&pool).unwrap_or(());
        }
    }

    actix_rt::spawn(_update_db(pool.clone()));

    let port = config.port;

    HttpServer::new(move || {
        App::new()
            .data(AppState { db: pool.clone() })
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(config.secret_key.as_bytes())
                    .name("auth")
                    .path("/")
                    .domain(config.domain.as_str())
                    .max_age_time(Duration::days(1))
                    .secure(false),
            ))
            .service(web::resource("/aws/index.html").route(web::get().to(sync_frontpage)))
    })
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .run()
    .await
    .expect("Failed to start app");
}
