use anyhow::Error;
use async_trait::async_trait;
use cached::{Cached, TimedCache};
use chrono::Local;
use futures::future::{try_join, try_join_all};
use lazy_static::lazy_static;
use rayon::{
    iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator},
    slice::ParallelSliceMut,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, RwLock};

use aws_app_lib::{
    aws_app_interface::{AwsAppInterface, INSTANCE_LIST},
    ec2_instance::AmiInfo,
    resource_type::ResourceType,
};

lazy_static! {
    static ref CACHE_UBUNTU_AMI: Mutex<TimedCache<String, Option<AmiInfo>>> =
        Mutex::new(TimedCache::with_lifespan(3600));
    static ref NOVNC_CHILDREN: RwLock<Vec<Child>> = RwLock::new(Vec::new());
}

macro_rules! get_cached {
    ($hash:ident, $mutex:expr, $call:expr) => {{
        let mut has_cache = false;

        let d = match $mutex.lock().await.cache_get(&$hash) {
            Some(d) => {
                has_cache = true;
                d.clone()
            }
            None => $call.await?,
        };
        if !has_cache {
            $mutex.lock().await.cache_set($hash.clone(), d.clone());
        }
        Ok(d)
    }};
}

#[async_trait]
pub trait HandleRequest<T> {
    type Result;
    async fn handle(&self, req: T) -> Self::Result;
}

#[async_trait]
impl HandleRequest<ResourceType> for AwsAppInterface {
    type Result = Result<Vec<String>, Error>;
    async fn handle(&self, req: ResourceType) -> Self::Result {
        let mut output = Vec::new();
        match req {
            ResourceType::Instances => {
                let result = list_instance(self).await?;
                if result.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead>
                    <tr>
                    <th>Instance Id</th><th>Public Hostname</th><th>State</th><th>Name</th>
                    <th>Instance Type</th><th>Created At</th><th>Availability Zone</th>
                    </tr>
                    </thead><tbody>"#
                        .to_string(),
                );
                output.extend_from_slice(&result);
                output.push("</tbody></table>".to_string());
            }
            ResourceType::Reserved => {
                let reserved = self.ec2.get_reserved_instances().await?;
                if reserved.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead>
                    <tr><th>Reserved Instance Id</th><th>Price</th><th>Instance Type</th>
                    <th>State</th><th>Availability Zone</th></tr>
                    </thead><tbody>"#
                        .to_string(),
                );
                let result: Vec<_> = reserved
                    .par_iter()
                    .map(|res| {
                        format!(
                            r#"<tr style="text-align: center;">
                                <td>{}</td><td>${:0.2}</td><td>{}</td><td>{}</td><td>{}</td>
                            </tr>"#,
                            res.id,
                            res.price,
                            res.instance_type,
                            res.state,
                            res.availability_zone
                                .as_ref()
                                .map_or_else(|| "", String::as_str)
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".to_string());
            }
            ResourceType::Spot => {
                let requests = self.ec2.get_spot_instance_requests().await?;
                if requests.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead>
                    <tr><th>Spot Request Id</th><th>Price</th><th>AMI</th><th>Instance Type</th>
                    <th>Spot Type</th><th>Status</th></tr>
                    </thead><tbody>"#
                        .to_string(),
                );
                let result: Vec<_> = requests
                    .par_iter()
                    .map(|req| {
                        format!(
                            r#"<tr style="text-align: center;">
                                <td>{}</td><td>${}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>
                            </tr>"#,
                            req.id,
                            req.price,
                            req.imageid,
                            req.instance_type,
                            req.spot_type,
                            req.status
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".to_string());
            }
            ResourceType::Ami => {
                let ubuntu_ami = async {
                    let hash = &self.config.ubuntu_release;
                    get_cached!(hash, CACHE_UBUNTU_AMI, self.ec2.get_latest_ubuntu_ami(hash))
                };

                let ami_tags = self.ec2.get_ami_tags();
                let (ubuntu_ami, mut ami_tags) = try_join(ubuntu_ami, ami_tags).await?;

                if ami_tags.is_empty() {
                    return Ok(Vec::new());
                }
                ami_tags.par_sort_by_key(|x| x.name.clone());
                if let Some(ami) = ubuntu_ami {
                    ami_tags.push(ami);
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead>
                    <tr><th></th><th></th><th>AMI</th><th>Name</th><th>State</th>
                    <th>Snapshot ID</th>
                    </tr>
                    </thead><tbody>"#
                        .to_string(),
                );
                let result: Vec<_> = ami_tags
                    .par_iter()
                    .map(|ami| {
                        format!(
                            r#"<tr style="text-align: center;">
                                <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>
                            </tr>"#,
                            format!(
                                r#"<input type="button" name="DeleteImage" value="DeleteImage"
                                    onclick="deleteImage('{}')">"#,
                                ami.id
                            ),
                            format!(
                                r#"<input type="button" name="Request" value="Request"
                                    onclick="buildSpotRequest('{}', null, null)">"#,
                                ami.id,
                            ),
                            ami.id,
                            ami.name,
                            ami.state,
                            ami.snapshot_ids.join(" ")
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".to_string());
            }
            ResourceType::Key => {
                let keys = self.ec2.get_all_key_pairs().await?;
                output.push(
                    r#"<table border="1" class="dataframe">
                        <thead><tr><th>Key Name</th><th>Key Fingerprint</th></tr></thead>
                        <tbody>"#
                        .to_string(),
                );
                let result: Vec<_> = keys
                    .into_par_iter()
                    .map(|(key, fingerprint)| {
                        format!(
                            r#"<tr style="text-align: center;"><td>{}</td><td>{}</td></tr>"#,
                            key, fingerprint
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".to_string());
            }
            ResourceType::Volume => {
                let volumes = self.ec2.get_all_volumes().await?;
                if volumes.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead><tr><th></th><th>Volume ID</th>
                    <th>Availability Zone</th><th>Size</th><th>IOPS</th><th>State</th><th>Tags</th>
                    </tr></thead><tbody>"#
                        .to_string(),
                );
                let result: Vec<_> = volumes
                    .par_iter()
                    .map(|vol| {
                        format!(
                            r#"<tr style="text-align: center;">
                                <td>{}</td><td>{}</td><td>{}</td><td>{} GB</td><td>{}</td><td>{}</td>
                                <td>{}</td></tr>"#,
                            if let Some("ddbolineinthecloud") = vol.tags.get("Name").map(String::as_str) {
                                "".to_string()
                            } else {
                                format!(
                                    r#"<input type="button" name="DeleteVolume" value="DeleteVolume"
                                        onclick="deleteVolume('{}')">"#,
                                    vol.id
                                )
                            },
                            vol.id,
                            vol.availability_zone,
                            vol.size,
                            vol.iops,
                            vol.state,
                            print_tags(&vol.tags)
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".to_string());
            }
            ResourceType::Snapshot => {
                let mut snapshots = self.ec2.get_all_snapshots().await?;
                if snapshots.is_empty() {
                    return Ok(Vec::new());
                }
                snapshots.par_sort_by_key(|k| {
                    k.tags
                        .get("Name")
                        .map_or_else(|| "".to_string(), ToString::to_string)
                });
                output.push(
                    r#"<table border="1" class="dataframe"><thead><tr>
                        <th></th><th>Snapshot ID</th><th>Size</th><th>State</th><th>Progress</th>
                        <th>Tags</th></tr></thead><tbody>"#
                        .to_string(),
                );
                let result: Vec<_> = snapshots
                    .par_iter()
                    .map(|snap| {
                        format!(
                            r#"<tr style="text-align: center;">
                                <td>{}</td><td>{}</td><td>{} GB</td><td>{}</td><td>{}</td>
                                <td>{}</td></tr>"#,
                            format!(
                                r#"<input type="button" name="DeleteSnapshot"
                                    value="DeleteSnapshot" onclick="deleteSnapshot('{}')">"#,
                                snap.id
                            ),
                            snap.id,
                            snap.volume_size,
                            snap.state,
                            snap.progress,
                            print_tags(&snap.tags)
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".to_string());
            }
            ResourceType::Ecr => {
                let repos = self.ecr.get_all_repositories().await?;
                if repos.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead><tr>
                        <th><input type="button" name="CleanupEcr" value="CleanupEcr"
                            onclick="cleanupEcrImages()"></th>
                            <th>ECR Repo</th><th>Tag</th><th>Digest</th><th>Pushed At</th>
                            <th>Image Size</th></tr></thead><tbody>"#
                        .to_string(),
                );

                let futures = repos.iter().map(|repo| get_ecr_images(self, repo));
                let results: Vec<_> = try_join_all(futures).await?.into_iter().flatten().collect();
                output.extend_from_slice(&results);
                output.push("</tbody></table>".to_string());
            }
            ResourceType::Script => {
                output.push(
                    r#"
                        <form action="javascript:createScript()">
                        <input type="text" name="script_filename" id="script_filename"/>
                        <input type="button" name="create_script" value="New"
                            onclick="createScript();"/></form>"#
                        .to_string(),
                );
                let result: Vec<_> = self
                    .get_all_scripts()?
                    .into_par_iter()
                    .map(|fname| {
                        format!(
                            "{} {} {} {}<br>",
                            format!(
                                r#"<input type="button" name="Edit" value="Edit"
                                onclick="editScript('{}')">"#,
                                fname,
                            ),
                            format!(
                                r#"<input type="button" name="Rm" value="Rm"
                                onclick="deleteScript('{}')">"#,
                                fname,
                            ),
                            format!(
                                r#"<input type="button" name="Request" value="Request"
                                onclick="buildSpotRequest(null, null, '{}')">"#,
                                fname,
                            ),
                            fname
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
            }
        };
        Ok(output)
    }
}

async fn list_instance(app: &AwsAppInterface) -> Result<Vec<String>, Error> {
    app.fill_instance_list().await?;

    let result: Vec<_> = INSTANCE_LIST
        .read()
        .await
        .par_iter()
        .map(|inst| {
            let name = inst
                .tags
                .get("Name")
                .cloned()
                .unwrap_or_else(|| "".to_string());
            format!(
                r#"
                    <tr style="text-align: center;">
                        <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>
                        <td>{}</td><td>{}</td><td>{}</td><td>{}</td>
                    </tr>
                "#,
                inst.id,
                inst.dns_name,
                inst.state,
                name,
                inst.instance_type,
                inst.launch_time.with_timezone(&Local),
                inst.availability_zone,
                if inst.state == "running" {
                    format!(
                        r#"<input type="button" name="Status" value="Status" {}>"#,
                        format!(r#"onclick="getStatus('{}')""#, inst.id)
                    )
                } else {
                    "".to_string()
                },
                if inst.state == "running" && name != "ddbolineinthecloud" {
                    format!(
                        r#"<input type="button" name="Terminate" value="Terminate" {}>"#,
                        format!(r#"onclick="terminateInstance('{}')""#, inst.id)
                    )
                } else {
                    "".to_string()
                }
            )
        })
        .collect();
    Ok(result)
}

async fn get_ecr_images(app: &AwsAppInterface, repo: &str) -> Result<Vec<String>, Error> {
    let images = app.ecr.get_all_images(&repo).await?;
    let lines: Vec<_> = images
        .par_iter()
        .map(|image| {
            format!(
                r#"<tr style="text-align: center;">
                <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:0.2} MB</td>
                </tr>"#,
                format!(
                    r#"<input type="button" name="DeleteEcrImage" value="DeleteEcrImage"
                        onclick="deleteEcrImage('{}', '{}')">"#,
                    repo, image.digest,
                ),
                repo,
                image.tags.get(0).map_or_else(|| "None", String::as_str),
                image.digest,
                image.pushed_at,
                image.image_size,
            )
        })
        .collect();
    Ok(lines)
}

fn print_tags(tags: &HashMap<String, String>) -> String {
    let results: Vec<_> = tags
        .par_iter()
        .map(|(k, v)| format!("{} = {}", k, v))
        .collect();
    results.join(", ")
}

#[derive(Serialize, Deserialize)]
pub struct TerminateRequest {
    pub instance: String,
}

#[async_trait]
impl HandleRequest<TerminateRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: TerminateRequest) -> Self::Result {
        self.terminate(&[req.instance]).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteImageRequest {
    pub ami: String,
}

#[async_trait]
impl HandleRequest<DeleteImageRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: DeleteImageRequest) -> Self::Result {
        self.delete_image(&req.ami).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteVolumeRequest {
    pub volid: String,
}

#[async_trait]
impl HandleRequest<DeleteVolumeRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: DeleteVolumeRequest) -> Self::Result {
        self.delete_ebs_volume(&req.volid).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteSnapshotRequest {
    pub snapid: String,
}

#[async_trait]
impl HandleRequest<DeleteSnapshotRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: DeleteSnapshotRequest) -> Self::Result {
        self.delete_ebs_snapshot(&req.snapid).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteEcrImageRequest {
    pub reponame: String,
    pub imageid: String,
}

#[async_trait]
impl HandleRequest<DeleteEcrImageRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: DeleteEcrImageRequest) -> Self::Result {
        self.ecr
            .delete_ecr_images(&req.reponame, &[req.imageid])
            .await
    }
}

pub struct CleanupEcrImagesRequest {}

#[async_trait]
impl HandleRequest<CleanupEcrImagesRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, _: CleanupEcrImagesRequest) -> Self::Result {
        self.ecr.cleanup_ecr_images().await
    }
}

#[derive(Serialize, Deserialize)]
pub struct StatusRequest {
    pub instance: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandRequest {
    pub instance: String,
    pub command: String,
}

pub struct NoVncStartRequest {}

#[async_trait]
impl HandleRequest<NoVncStartRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, _: NoVncStartRequest) -> Self::Result {
        let home_dir = dirs::home_dir().expect("No home directory");
        let x11vnc = Path::new("/usr/bin/x11vnc");
        // let vncserver = Path::new("/usr/bin/vncserver");
        let vncpwd = home_dir.join(".vnc/passwd");
        let websockify = Path::new("/usr/bin/websockify");
        let certdir = Path::new("/etc/letsencrypt/live/").join(&self.config.domain);
        let cert = certdir.join("fullchain.pem");
        let key = certdir.join("privkey.pem");

        if x11vnc.exists() {
            if let Some(novnc_path) = &self.config.novnc_path {
                let x11vnc_command = Command::new(&x11vnc)
                    .args(&[
                        "-safer",
                        "-rfbauth",
                        &vncpwd.to_string_lossy(),
                        "-forever",
                        "-display",
                        ":0",
                    ])
                    .kill_on_drop(true)
                    .spawn()?;
                let websockify_command = Command::new("sudo")
                    .args(&[
                        &websockify.to_string_lossy(),
                        "8787",
                        "--ssl-only",
                        "--web",
                        novnc_path,
                        "--cert",
                        &cert.to_string_lossy(),
                        "--key",
                        &key.to_string_lossy(),
                        "localhost:5900",
                    ])
                    .kill_on_drop(true)
                    .spawn()?;

                let mut children = NOVNC_CHILDREN.write().await;
                children.push(x11vnc_command);
                children.push(websockify_command);
            }
        }
        Ok(())
    }
}

pub struct NoVncStopRequest {}

#[async_trait]
impl HandleRequest<NoVncStopRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, _: NoVncStopRequest) -> Self::Result {
        let mut children = NOVNC_CHILDREN.write().await;
        children.clear();

        let mut kill = Command::new("sudo");
        kill.args(&["kill", "-9"]);
        let ids = get_websock_pids().await?.into_iter().map(|x| x.to_string());
        kill.args(ids);
        let kill = kill.spawn()?;
        kill.wait_with_output().await?;
        Ok(())
    }
}

pub async fn get_websock_pids() -> Result<Vec<usize>, Error> {
    let websock = Command::new("ps")
        .args(&["-eF"])
        .stdout(Stdio::piped())
        .spawn()?;
    let output = websock.wait_with_output().await?;
    let output = String::from_utf8(output.stdout)?;
    let result: Vec<_> = output
        .split('\n')
        .filter_map(|s| {
            if s.contains("websockify") {
                s.split_whitespace().nth(1).and_then(|x| x.parse().ok())
            } else {
                None
            }
        })
        .collect();
    Ok(result)
}

pub struct NoVncStatusRequest {}

#[async_trait]
impl HandleRequest<NoVncStatusRequest> for AwsAppInterface {
    type Result = usize;
    async fn handle(&self, _: NoVncStatusRequest) -> Self::Result {
        NOVNC_CHILDREN.read().await.len()
    }
}
