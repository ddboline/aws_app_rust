use anyhow::Error;
use rweb::Filter;
use std::{net::SocketAddr, time::Duration};
use tokio::time::interval;

use aws_app_lib::{
    aws_app_interface::AwsAppInterface, config::Config, novnc_instance::NoVncInstance,
    pgpool::PgPool,
};

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
    pub novnc: NoVncInstance,
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
        novnc: NoVncInstance::new(),
    };

    tokio::task::spawn(_update_db(app.aws.pool.clone()));

    let frontpage_path = sync_frontpage(app.clone()).boxed();
    let list_path = list(app.clone()).boxed();
    let terminate_path = terminate(app.clone()).boxed();
    let create_image_path = create_image(app.clone()).boxed();
    let delete_image_path = delete_image(app.clone()).boxed();
    let delete_volume_path = delete_volume(app.clone()).boxed();
    let modify_volume_path = modify_volume(app.clone()).boxed();
    let delete_snapshot_path = delete_snapshot(app.clone()).boxed();
    let create_snapshot_path = create_snapshot(app.clone()).boxed();
    let tag_item_path = tag_item(app.clone()).boxed();
    let delete_ecr_image_path = delete_ecr_image(app.clone()).boxed();
    let cleanup_ecr_images_path = cleanup_ecr_images(app.clone()).boxed();
    let edit_script_path = edit_script(app.clone()).boxed();
    let replace_script_path = replace_script(app.clone()).boxed();
    let delete_script_path = delete_script(app.clone()).boxed();
    let create_user_path = create_user(app.clone()).boxed();
    let delete_user_path = delete_user(app.clone()).boxed();
    let add_user_to_group_path = add_user_to_group(app.clone()).boxed();
    let remove_user_from_group_path = remove_user_from_group(app.clone()).boxed();
    let create_access_key_path = create_access_key(app.clone()).boxed();
    let delete_access_key_path = delete_access_key(app.clone()).boxed();
    let build_spot_request_path = build_spot_request(app.clone()).boxed();
    let request_spot_path = request_spot(app.clone()).boxed();
    let cancel_spot_path = cancel_spot(app.clone()).boxed();
    let get_prices_path = get_prices(app.clone()).boxed();
    let update_path = update(app.clone()).boxed();
    let instance_status_path = instance_status(app.clone()).boxed();
    let command_path = command(app.clone()).boxed();
    let get_instances_path = get_instances(app.clone()).boxed();
    let user_path = user().boxed();
    let novnc_launcher_path = novnc_launcher(app.clone()).boxed();
    let novnc_status_path = novnc_status(app.clone()).boxed();
    let novnc_shutdown_path = novnc_shutdown(app.clone()).boxed();
    let update_dns_name_path = update_dns_name(app.clone()).boxed();

    let novnc_scope = novnc_launcher_path
        .or(novnc_status_path)
        .or(novnc_shutdown_path)
        .boxed();

    let aws_path = frontpage_path
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
        .or(update_dns_name_path)
        .boxed();
    let routes = aws_path.recover(error_response);
    let addr: SocketAddr = format!("127.0.0.1:{}", config.port).parse()?;
    rweb::serve(routes).bind(addr).await;
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
