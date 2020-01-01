use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{web, App, HttpServer};
use chrono::Duration;
use std::time;
use tokio::time::interval;

use aws_app_lib::aws_app_interface::AwsAppInterface;
use aws_app_lib::config::Config;
use aws_app_lib::pgpool::PgPool;

use super::logged_user::AUTHORIZED_USERS;
use super::routes::{
    cleanup_ecr_images, delete_ecr_image, delete_image, delete_snapshot, delete_volume, list,
    sync_frontpage, terminate,
};

pub struct AppState {
    pub aws: AwsAppInterface,
}

pub async fn start_app() {
    let config = Config::init_config().expect("Failed to load config");
    let pool = PgPool::new(&config.database_url);
    let aws = AwsAppInterface::new(config, pool);

    async fn _update_db(pool: PgPool) {
        let mut i = interval(time::Duration::from_secs(60));
        loop {
            i.tick().await;
            AUTHORIZED_USERS.fill_from_db(&pool).unwrap_or(());
        }
    }

    actix_rt::spawn(_update_db(aws.pool.clone()));

    let port = aws.config.port;

    HttpServer::new(move || {
        App::new()
            .data(AppState { aws: aws.clone() })
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(aws.config.secret_key.as_bytes())
                    .name("auth")
                    .path("/")
                    .domain(aws.config.domain.as_str())
                    .max_age_time(Duration::days(1))
                    .secure(false),
            ))
            .service(web::resource("/aws/index.html").route(web::get().to(sync_frontpage)))
            .service(web::resource("/aws/list").route(web::get().to(list)))
            .service(web::resource("/aws/terminate").route(web::get().to(terminate)))
            .service(web::resource("/aws/delete_image").route(web::get().to(delete_image)))
            .service(web::resource("/aws/delete_volume").route(web::get().to(delete_volume)))
            .service(web::resource("/aws/delete_snapshot").route(web::get().to(delete_snapshot)))
            .service(web::resource("/aws/delete_ecr_image").route(web::get().to(delete_ecr_image)))
            .service(
                web::resource("/aws/cleanup_ecr_images").route(web::get().to(cleanup_ecr_images)),
            )
    })
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .run()
    .await
    .expect("Failed to start app");
}