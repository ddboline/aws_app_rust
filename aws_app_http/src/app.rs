use anyhow::Error;
use std::{net::SocketAddr, time::Duration};
use tokio::time::interval;
use warp::Filter;

use aws_app_lib::{aws_app_interface::AwsAppInterface, config::Config, pgpool::PgPool};

use super::{
    errors::error_response,
    logged_user::{fill_from_db, get_secrets, TRIGGER_DB_UPDATE},
    routes::{
        add_user_to_group, build_spot_request, cancel_spot, cleanup_ecr_images, command,
        create_access_key, create_image, create_snapshot, create_user, delete_access_key,
        delete_ecr_image, delete_image, delete_script, delete_snapshot, delete_user, delete_volume,
        edit_script, get_instances, get_prices, instance_status, list, modify_volume,
        novnc_launcher, novnc_shutdown, novnc_status, remove_user_from_group, replace_script,
        request_spot, sync_frontpage, tag_item, terminate, update, update_dns_name, user,
    },
};

#[derive(Clone)]
pub struct AppState {
    pub aws: AwsAppInterface,
}

pub async fn start_app() -> Result<(), Error> {
    let config = Config::init_config()?;
    get_secrets(&config.secret_path, &config.jwt_secret_path).await?;
    run_app(&config).await
}

async fn run_app(config: &Config) -> Result<(), Error> {
    async fn _update_db(pool: PgPool) {
        let mut i = interval(Duration::from_secs(60));
        loop {
            fill_from_db(&pool).await.unwrap_or(());
            i.tick().await;
        }
    }

    TRIGGER_DB_UPDATE.set();

    let pool = PgPool::new(&config.database_url);
    let app = AppState {
        aws: AwsAppInterface::new(config.clone(), pool),
    };

    tokio::task::spawn(_update_db(app.aws.pool.clone()));

    let data = warp::any().map(move || app.clone());

    let frontpage_path = warp::path("index.html")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(sync_frontpage)
        .boxed();
    let list_path = warp::path("list")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(list)
        .boxed();
    let terminate_path = warp::path("terminate")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(terminate)
        .boxed();
    let create_image_path = warp::path("create_image")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(create_image)
        .boxed();
    let delete_image_path = warp::path("delete_image")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(delete_image)
        .boxed();
    let delete_volume_path = warp::path("delete_volume")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(delete_volume)
        .boxed();
    let modify_volume_path = warp::path("modify_volume")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(modify_volume)
        .boxed();
    let delete_snapshot_path = warp::path("delete_snapshot")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(delete_snapshot)
        .boxed();
    let create_snapshot_path = warp::path("create_snapshot")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(create_snapshot)
        .boxed();
    let tag_item_path = warp::path("tag_item")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(tag_item)
        .boxed();
    let delete_ecr_image_path = warp::path("delete_ecr_image")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(delete_ecr_image)
        .boxed();
    let cleanup_ecr_images_path = warp::path("cleanup_ecr_images")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(cleanup_ecr_images)
        .boxed();
    let edit_script_path = warp::path("edit_script")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(edit_script)
        .boxed();
    let replace_script_path = warp::path("replace_script")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(replace_script)
        .boxed();
    let delete_script_path = warp::path("delete_script")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(delete_script)
        .boxed();
    let create_user_path = warp::path("create_user")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(create_user)
        .boxed();
    let delete_user_path = warp::path("delete_user")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(delete_user)
        .boxed();
    let add_user_to_group_path = warp::path("add_user_to_group")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(add_user_to_group)
        .boxed();
    let remove_user_from_group_path = warp::path("remove_user_from_group")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(remove_user_from_group)
        .boxed();
    let create_access_key_path = warp::path("create_access_key")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(create_access_key)
        .boxed();
    let delete_access_key_path = warp::path("delete_access_key")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(delete_access_key)
        .boxed();
    let build_spot_request_path = warp::path("build_spot_request")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(build_spot_request)
        .boxed();
    let request_spot_path = warp::path("request_spot")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(request_spot)
        .boxed();
    let cancel_spot_path = warp::path("cancel_spot")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(cancel_spot)
        .boxed();
    let get_prices_path = warp::path("prices")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(get_prices)
        .boxed();
    let update_path = warp::path("update")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(update)
        .boxed();
    let instance_status_path = warp::path("instance_status")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(instance_status)
        .boxed();
    let command_path = warp::path("command")
        .and(warp::path::end())
        .and(warp::post())
        .and(warp::body::json())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(command)
        .boxed();
    let get_instances_path = warp::path("instances")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(get_instances)
        .boxed();
    let user_path = warp::path("user")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and_then(user)
        .boxed();
    let novnc_launcher_path = warp::path("start")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(novnc_launcher)
        .boxed();
    let novnc_status_path = warp::path("status")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(novnc_status)
        .boxed();
    let novnc_shutdown_path = warp::path("stop")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(novnc_shutdown)
        .boxed();
    let update_dns_name_path = warp::path("update_dns_name")
        .and(warp::path::end())
        .and(warp::get())
        .and(warp::query())
        .and(warp::cookie("jwt"))
        .and(data.clone())
        .and_then(update_dns_name)
        .boxed();

    let novnc_scope = warp::path("novnc")
        .and(
            novnc_launcher_path
                .or(novnc_status_path)
                .or(novnc_shutdown_path),
        )
        .boxed();

    let aws_path = warp::path("aws")
        .and(
            frontpage_path
                .or(list_path)
                .or(terminate_path)
                .or(create_image_path)
                .or(delete_image_path)
                .or(delete_volume_path)
                .or(modify_volume_path)
                .or(delete_snapshot_path)
                .or(create_snapshot_path)
                .or(tag_item_path)
                .or(delete_ecr_image_path)
                .or(cleanup_ecr_images_path)
                .or(edit_script_path)
                .or(replace_script_path)
                .or(delete_script_path)
                .or(create_user_path)
                .or(delete_user_path)
                .or(add_user_to_group_path)
                .or(remove_user_from_group_path)
                .or(create_access_key_path)
                .or(delete_access_key_path)
                .or(build_spot_request_path)
                .or(request_spot_path)
                .or(cancel_spot_path)
                .or(get_prices_path)
                .or(update_path)
                .or(instance_status_path)
                .or(command_path)
                .or(get_instances_path)
                .or(user_path)
                .or(novnc_scope)
                .or(update_dns_name_path),
        )
        .boxed();
    let routes = aws_path.recover(error_response);
    let addr: SocketAddr = format!("127.0.0.1:{}", config.port).parse()?;
    warp::serve(routes).bind(addr).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use maplit::hashmap;
    use std::env::{remove_var, set_var};

    use auth_server_http::app::run_test_app;

    use aws_app_lib::{config::Config, resource_type::ResourceType};

    use crate::{
        app::run_app,
        logged_user::{get_random_key, JWT_SECRET, KEY_LENGTH, SECRET_KEY},
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn test_app() -> Result<(), Error> {
        set_var("TESTENV", "true");

        let email = "test_aws_app_user@localhost";
        let password = "abc123xyz8675309";

        let auth_port: u32 = 54321;
        set_var("PORT", auth_port.to_string());
        set_var("DOMAIN", "localhost");

        let config = auth_server_lib::config::Config::init_config()?;

        let mut secret_key = [0u8; KEY_LENGTH];
        secret_key.copy_from_slice(&get_random_key());

        JWT_SECRET.set(secret_key);
        SECRET_KEY.set(secret_key);

        println!("spawning auth");
        tokio::task::spawn(async move { run_test_app(config).await.unwrap() });

        let test_port: u32 = 12345;
        set_var("PORT", test_port.to_string());
        let config = Config::init_config()?;

        println!("spawning aws");
        tokio::task::spawn(async move {
            env_logger::init();
            run_app(&config).await.unwrap()
        });
        println!("sleeping");
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;

        let client = reqwest::Client::builder().cookie_store(true).build()?;
        let url = format!("http://localhost:{}/api/auth", auth_port);
        let data = hashmap! {
            "email" => &email,
            "password" => &password,
        };
        let result = client
            .post(&url)
            .json(&data)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        println!("{}", result);

        let url = format!("http://localhost:{}/aws/index.html", test_port);
        let result = client
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        assert!(result.len() > 0);
        assert!(result.contains("Instance Id"));

        for (rtype, substr) in &[
            (ResourceType::Instances, "Instance Id"),
            (ResourceType::Reserved, "Reserved Instance Id"),
            (ResourceType::Spot, "Spot Request Id"),
            (ResourceType::Ami, "Snapshot ID"),
            (ResourceType::Volume, "Volume ID"),
            (ResourceType::Snapshot, "Snapshot ID"),
            (ResourceType::Ecr, "ECR Repo"),
            (ResourceType::Key, "Key Name"),
            (ResourceType::Script, "createScript"),
            (ResourceType::User, "User ID"),
            (ResourceType::Group, "Group ID"),
            (ResourceType::AccessKey, "Key ID"),
        ] {
            let url = format!("http://localhost:{}/aws/list?resource={}", test_port, rtype);
            let result = client
                .get(&url)
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            if result.len() > 0 {
                let cond = result.contains(substr);
                if !cond {
                    println!("{} {}", rtype, result);
                }
                assert!(cond);
            }
        }

        remove_var("TESTENV");

        Ok(())
    }
}
