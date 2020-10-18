use actix_identity::{CookieIdentityPolicy, IdentityService};
use actix_web::{middleware::Compress, web, App, HttpServer};
use anyhow::Error;
use lazy_static::lazy_static;
use std::time::Duration;
use tokio::time::interval;
use stack_string::StackString;

use aws_app_lib::{aws_app_interface::AwsAppInterface, config::Config, pgpool::PgPool};

use super::{
    logged_user::{fill_from_db, get_secrets, SECRET_KEY, TRIGGER_DB_UPDATE, KEY_LENGTH},
    routes::{
        add_user_to_group, build_spot_request, cancel_spot, cleanup_ecr_images, command,
        create_access_key, create_image, create_snapshot, create_user, delete_access_key,
        delete_ecr_image, delete_image, delete_script, delete_snapshot, delete_user, delete_volume,
        edit_script, get_instances, get_prices, list, modify_volume, novnc_launcher,
        novnc_shutdown, novnc_status, remove_user_from_group, replace_script, request_spot, status,
        sync_frontpage, tag_item, terminate, update, user,
    },
};

lazy_static! {
    pub static ref CONFIG: Config = Config::init_config().expect("Failed to init config");
}

pub struct AppState {
    pub aws: AwsAppInterface,
}

pub async fn start_app() -> Result<(), Error> {
    let port = CONFIG.port;
    get_secrets(&CONFIG.secret_path, &CONFIG.jwt_secret_path).await?;
    run_app(&CONFIG, port, SECRET_KEY.get(), CONFIG.domain.clone()).await
}

async fn run_app(config: &Config, port: u32, cookie_secret: [u8; KEY_LENGTH], domain: StackString) -> Result<(), Error> {
    async fn _update_db(pool: PgPool) {
        let mut i = interval(Duration::from_secs(60));
        loop {
            fill_from_db(&pool).await.unwrap_or(());
            i.tick().await;
        }
    }

    TRIGGER_DB_UPDATE.set();

    let pool = PgPool::new(&config.database_url);
    let aws = AwsAppInterface::new(config.clone(), pool);

    actix_rt::spawn(_update_db(aws.pool.clone()));

    HttpServer::new(move || {
        App::new()
            .data(AppState { aws: aws.clone() })
            .wrap(Compress::default())
            .wrap(IdentityService::new(
                CookieIdentityPolicy::new(&cookie_secret)
                    .name("auth")
                    .path("/")
                    .domain(domain.as_str())
                    .max_age(24 * 3600)
                    .secure(false),
            ))
            .service(
                web::scope("/aws")
                    .service(web::resource("/index.html").route(web::get().to(sync_frontpage)))
                    .service(web::resource("/list").route(web::get().to(list)))
                    .service(web::resource("/terminate").route(web::get().to(terminate)))
                    .service(web::resource("/create_image").route(web::get().to(create_image)))
                    .service(web::resource("/delete_image").route(web::get().to(delete_image)))
                    .service(web::resource("/delete_volume").route(web::get().to(delete_volume)))
                    .service(web::resource("/modify_volume").route(web::get().to(modify_volume)))
                    .service(
                        web::resource("/delete_snapshot").route(web::get().to(delete_snapshot)),
                    )
                    .service(
                        web::resource("/create_snapshot").route(web::get().to(create_snapshot)),
                    )
                    .service(web::resource("/tag_item").route(web::get().to(tag_item)))
                    .service(
                        web::resource("/delete_ecr_image").route(web::get().to(delete_ecr_image)),
                    )
                    .service(
                        web::resource("/cleanup_ecr_images")
                            .route(web::get().to(cleanup_ecr_images)),
                    )
                    .service(web::resource("/edit_script").route(web::get().to(edit_script)))
                    .service(web::resource("/replace_script").route(web::post().to(replace_script)))
                    .service(web::resource("/delete_script").route(web::get().to(delete_script)))
                    .service(web::resource("/create_user").route(web::get().to(create_user)))
                    .service(web::resource("/delete_user").route(web::get().to(delete_user)))
                    .service(
                        web::resource("/add_user_to_group").route(web::get().to(add_user_to_group)),
                    )
                    .service(
                        web::resource("/remove_user_from_group")
                            .route(web::get().to(remove_user_from_group)),
                    )
                    .service(
                        web::resource("/create_access_key").route(web::get().to(create_access_key)),
                    )
                    .service(
                        web::resource("/delete_access_key").route(web::get().to(delete_access_key)),
                    )
                    .service(
                        web::resource("/build_spot_request")
                            .route(web::get().to(build_spot_request)),
                    )
                    .service(web::resource("/request_spot").route(web::post().to(request_spot)))
                    .service(web::resource("/cancel_spot").route(web::get().to(cancel_spot)))
                    .service(web::resource("/prices").route(web::get().to(get_prices)))
                    .service(web::resource("/update").route(web::get().to(update)))
                    .service(web::resource("/status").route(web::get().to(status)))
                    .service(web::resource("/command").route(web::post().to(command)))
                    .service(web::resource("/instances").route(web::get().to(get_instances)))
                    .service(
                        web::scope("/novnc")
                            .service(web::resource("/start").route(web::get().to(novnc_launcher)))
                            .service(web::resource("/status").route(web::get().to(novnc_status)))
                            .service(web::resource("/stop").route(web::get().to(novnc_shutdown))),
                    )
                    .service(web::resource("/user").route(web::get().to(user))),
            )
    })
    .bind(&format!("127.0.0.1:{}", port))
    .unwrap_or_else(|_| panic!("Failed to bind to port {}", port))
    .run()
    .await
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::env::{set_var, remove_var};
    use anyhow::Error;

    use aws_app_lib::config::Config;

    use crate::logged_user::{get_random_key, KEY_LENGTH};
    use crate::app::run_app;

    #[actix_rt::test]
    async fn test_app() -> Result<(), Error> {
        set_var("TESTENV", "true");
        remove_var("TESTENV");

        let config = Config::init_config()?;

        let mut secret_key = [0u8; KEY_LENGTH];
        secret_key.copy_from_slice(&get_random_key());

        let test_port: u32 = 12345;
        actix_rt::spawn(async move {run_app(&config, test_port, secret_key, "localhost".into()).await.unwrap()});
        actix_rt::time::delay_for(std::time::Duration::from_secs(10)).await;

        let url = format!("http://localhost:{}/aws/index.html", test_port);
        let result = reqwest::get(&url).await?.error_for_status()?.text().await?;
        println!("{}", result);
        assert!(result.len() > 0);
        assert!(result.contains("InstanceId"));
        Ok(())
    }
}