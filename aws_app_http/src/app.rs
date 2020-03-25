use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{web, App, HttpServer};
use chrono::Duration;
use std::time;
use tokio::time::interval;

use aws_app_lib::{aws_app_interface::AwsAppInterface, config::Config, pgpool::PgPool};

use super::{
    logged_user::{fill_from_db, TRIGGER_DB_UPDATE},
    routes::{
        build_spot_request, cancel_spot, cleanup_ecr_images, command, delete_ecr_image,
        delete_image, delete_script, delete_snapshot, delete_volume, edit_script, get_instances,
        get_prices, list, novnc_launcher, novnc_shutdown, novnc_status, replace_script,
        request_spot, status, sync_frontpage, terminate, update, user,
    },
};

pub struct AppState {
    pub aws: AwsAppInterface,
}

pub async fn start_app() {
    async fn _update_db(pool: PgPool) {
        let mut i = interval(time::Duration::from_secs(60));
        loop {
            i.tick().await;
            fill_from_db(&pool).await.unwrap_or(());
        }
    }
    TRIGGER_DB_UPDATE.set();

    let config = Config::init_config().expect("Failed to load config");
    let pool = PgPool::new(&config.database_url);
    let aws = AwsAppInterface::new(config, pool);

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
            .service(web::resource("/aws/edit_script").route(web::get().to(edit_script)))
            .service(web::resource("/aws/replace_script").route(web::post().to(replace_script)))
            .service(web::resource("/aws/delete_script").route(web::get().to(delete_script)))
            .service(
                web::resource("/aws/build_spot_request").route(web::get().to(build_spot_request)),
            )
            .service(web::resource("/aws/request_spot").route(web::post().to(request_spot)))
            .service(web::resource("/aws/cancel_spot").route(web::get().to(cancel_spot)))
            .service(web::resource("/aws/prices").route(web::get().to(get_prices)))
            .service(web::resource("/aws/update").route(web::get().to(update)))
            .service(web::resource("/aws/status").route(web::get().to(status)))
            .service(web::resource("/aws/command").route(web::post().to(command)))
            .service(web::resource("/aws/instances").route(web::get().to(get_instances)))
            .service(web::resource("/aws/novnc/start").route(web::get().to(novnc_launcher)))
            .service(web::resource("/aws/novnc/status").route(web::get().to(novnc_status)))
            .service(web::resource("/aws/novnc/stop").route(web::get().to(novnc_shutdown)))
            .service(web::resource("/aws/user").route(web::get().to(user)))
    })
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .run()
    .await
    .expect("Failed to start app");
}
