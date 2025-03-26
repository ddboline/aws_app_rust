use axum::http::{Method, StatusCode};
use stack_string::format_sstr;
use std::{convert::TryInto, net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, task::spawn, time::interval};
use tower_http::cors::{Any, CorsLayer};
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use aws_app_lib::{
    aws_app_interface::AwsAppInterface, config::Config, errors::AwslibError,
    novnc_instance::NoVncInstance, pgpool::PgPool,
};

use super::{
    errors::ServiceError,
    logged_user::{fill_from_db, get_secrets},
    routes::{ApiDoc, get_aws_path},
};

#[derive(Clone)]
pub struct AppState {
    pub aws: AwsAppInterface,
    pub novnc: NoVncInstance,
}

/// # Errors
/// Returns error if config fails, `get_secrets` fails, or app fails to run
pub async fn start_app() -> Result<(), ServiceError> {
    let config = Config::init_config()?;
    get_secrets(&config.secret_path, &config.jwt_secret_path).await?;
    run_app(&config, config.port).await
}

async fn run_app(config: &Config, port: u32) -> Result<(), ServiceError> {
    async fn update_db(pool: PgPool) {
        let mut i = interval(Duration::from_secs(60));
        loop {
            fill_from_db(&pool).await.unwrap_or(());
            i.tick().await;
        }
    }

    let pool = PgPool::new(&config.database_url)?;
    let sdk_config = aws_config::load_from_env().await;
    let app = AppState {
        aws: AwsAppInterface::new(config.clone(), &sdk_config, pool),
        novnc: NoVncInstance::new(),
    };

    let update_handle = spawn(update_db(app.aws.pool.clone()));

    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST])
        .allow_headers(["content-type".try_into()?, "jwt".try_into()?])
        .allow_origin(Any);

    let (router, api) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(get_aws_path(&app))
        .split_for_parts();

    let spec_json = serde_json::to_string_pretty(&api)?;
    let spec_yaml = serde_yml::to_string(&api)?;

    let router = router
        .route(
            "/aws/openapi/json",
            axum::routing::get(|| async move {
                (
                    StatusCode::OK,
                    [("content-type", "application/json")],
                    spec_json,
                )
            }),
        )
        .route(
            "/aws/openapi/yaml",
            axum::routing::get(|| async move {
                (StatusCode::OK, [("content-type", "text/yaml")], spec_yaml)
            }),
        )
        .layer(cors);

    let host = &config.host;
    let addr: SocketAddr = format_sstr!("{host}:{port}").parse()?;
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, router.into_make_service()).await?;

    update_handle.await.map_err(Into::<AwslibError>::into)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use maplit::hashmap;
    use stack_string::format_sstr;
    use std::{
        env::{remove_var, set_var},
        time::Duration,
    };
    use tokio::{task::spawn, time::sleep};

    use auth_server_http::app::run_test_app;

    use aws_app_lib::{config::Config, errors::AwslibError as Error, resource_type::ResourceType};

    use crate::{
        app::run_app,
        logged_user::{JWT_SECRET, KEY_LENGTH, SECRET_KEY, get_random_key},
    };

    #[tokio::test(flavor = "multi_thread")]
    async fn test_app() -> Result<(), Error> {
        unsafe {
            set_var("TESTENV", "true");
        }

        let email = "test_aws_app_user@localhost";
        let password = "abc123xyz8675309";

        let auth_port: u32 = 54321;
        unsafe {
            set_var("PORT", auth_port.to_string());
            set_var("DOMAIN", "localhost");
        }

        let config = auth_server_lib::config::Config::init_config().unwrap();

        let mut secret_key = [0u8; KEY_LENGTH];
        secret_key.copy_from_slice(&get_random_key());

        JWT_SECRET.set(secret_key);
        SECRET_KEY.set(secret_key);

        println!("spawning auth");
        let test_app_handle = spawn(async move { run_test_app(config).await.unwrap() });

        let test_port: u32 = 12345;
        let config = Config::init_config()?;

        println!("spawning aws");
        let app_handle = spawn(async move {
            env_logger::init();
            run_app(&config, test_port).await.unwrap()
        });
        println!("sleeping");
        sleep(Duration::from_secs(10)).await;

        let client = reqwest::Client::builder().cookie_store(true).build()?;
        let url = format_sstr!("http://localhost:{auth_port}/api/auth");
        let data = hashmap! {
            "email" => &email,
            "password" => &password,
        };
        let result = client
            .post(url.as_str())
            .json(&data)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        println!("{}", result);

        let url = format_sstr!("http://localhost:{test_port}/aws/index.html");
        let result = client
            .get(url.as_str())
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
            let url = format_sstr!("http://localhost:{test_port}/aws/list?resource={rtype}");
            let result = client
                .get(url.as_str())
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

        let url = format_sstr!("http://localhost:{test_port}/aws/openapi/yaml");
        let spec_yaml = client
            .get(url.as_str())
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        tokio::fs::write("../scripts/openapi.yaml", &spec_yaml).await?;

        unsafe {
            remove_var("TESTENV");
        }
        test_app_handle.abort();
        app_handle.abort();

        Ok(())
    }
}
