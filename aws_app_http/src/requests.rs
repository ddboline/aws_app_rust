use anyhow::Error;
use async_trait::async_trait;
use cached::{Cached, SizedCache};
use chrono::{DateTime, Duration, Local, Utc};
use futures::future::try_join_all;
use itertools::Itertools;
use lazy_static::lazy_static;
use log::debug;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use stack_string::StackString;
use std::{
    collections::HashMap, fmt::Display, future::Future, ops::Deref, path::Path, process::Stdio,
};
use tokio::{
    process::{Child, Command},
    sync::{Mutex, RwLock},
    try_join,
};

use aws_app_lib::{
    aws_app_interface::{AwsAppInterface, INSTANCE_LIST},
    ec2_instance::AmiInfo,
    resource_type::ResourceType,
};

type AmiInfoValue = (DateTime<Utc>, Option<AmiInfo>);

lazy_static! {
    static ref CACHE_UBUNTU_AMI: InfoCache = InfoCache::default();
    static ref NOVNC_CHILDREN: RwLock<Vec<Child>> = RwLock::new(Vec::new());
}

struct InfoCache(Mutex<SizedCache<StackString, AmiInfoValue>>);

impl Default for InfoCache {
    fn default() -> Self {
        Self(Mutex::new(SizedCache::with_size(10)))
    }
}

impl Deref for InfoCache {
    type Target = Mutex<SizedCache<StackString, AmiInfoValue>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl InfoCache {
    async fn get_cached<F>(&self, hash: &str, call: F) -> Result<Option<AmiInfo>, Error>
    where
        F: Future<Output = Result<Option<AmiInfo>, Error>>,
    {
        let mut has_cache = false;
        let d = match self.lock().await.cache_get(&hash.into()) {
            Some((t, d)) => {
                if *t < Utc::now() - Duration::hours(1) {
                    call.await?
                } else {
                    has_cache = true;
                    d.clone()
                }
            }
            None => call.await?,
        };
        if !has_cache {
            self.lock()
                .await
                .cache_set(hash.into(), (Utc::now(), d.clone()));
        }
        Ok(d)
    }
}

#[async_trait]
pub trait HandleRequest<T> {
    type Result;
    async fn handle(&self, req: T) -> Self::Result;
}

#[async_trait]
impl HandleRequest<ResourceType> for AwsAppInterface {
    type Result = Result<Vec<StackString>, Error>;
    async fn handle(&self, req: ResourceType) -> Self::Result {
        let mut output: Vec<StackString> = Vec::new();
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
                        .into(),
                );
                output.extend_from_slice(&result);
                output.push("</tbody></table>".into());
            }
            ResourceType::Reserved => {
                let reserved: Vec<_> = self.ec2.get_reserved_instances().await?.collect();
                if reserved.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead>
                    <tr><th>Reserved Instance Id</th><th>Price</th><th>Instance Type</th>
                    <th>State</th><th>Availability Zone</th></tr>
                    </thead><tbody>"#
                        .into(),
                );
                let result: Vec<_> = reserved
                    .iter()
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
                                .map_or_else(|| "", StackString::as_str)
                        )
                        .into()
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".into());
            }
            ResourceType::Spot => {
                let requests: Vec<_> = self.ec2.get_spot_instance_requests().await?.collect();
                if requests.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead>
                    <tr><th>Spot Request Id</th><th>Price</th><th>AMI</th><th>Instance Type</th>
                    <th>Spot Type</th><th>Status</th></tr>
                    </thead><tbody>"#
                        .into(),
                );
                let result: Vec<_> = requests
                    .iter()
                    .map(|req| {
                        format!(
                            r#"<tr style="text-align: center;">
                                <td>{}</td><td>${}</td><td>{}</td><td>{}</td>
                                <td>{}</td><td>{}</td><td>{}</td>
                            </tr>"#,
                            req.id,
                            req.price,
                            req.imageid,
                            req.instance_type,
                            req.spot_type,
                            req.status,
                            match req.status.as_str() {
                                "pending" | "pending-fulfillment" => format!(
                                    r#"<input type="button" name="cancel" value="Cancel"
                                        onclick="cancelSpotRequest('{}')">"#,
                                    req.id
                                ),
                                _ => "".to_string(),
                            }
                        )
                        .into()
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".into());
            }
            ResourceType::Ami => {
                let ubuntu_ami = async {
                    let hash = self.config.ubuntu_release.as_str();
                    CACHE_UBUNTU_AMI
                        .get_cached(hash, self.ec2.get_latest_ubuntu_ami(hash))
                        .await
                };

                let ami_tags = self.ec2.get_ami_tags();
                let (ubuntu_ami, ami_tags) = try_join!(ubuntu_ami, ami_tags)?;
                let mut ami_tags: Vec<_> = ami_tags.collect();

                if ami_tags.is_empty() {
                    return Ok(Vec::new());
                }
                ami_tags.sort_by(|x, y| x.name.cmp(&y.name));
                if let Some(ami) = ubuntu_ami {
                    ami_tags.push(ami);
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead>
                    <tr><th></th><th></th><th>AMI</th><th>Name</th><th>State</th>
                    <th>Snapshot ID</th>
                    </tr>
                    </thead><tbody>"#
                        .into(),
                );
                let result: Vec<_> = ami_tags
                    .iter()
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
                        .into()
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".into());
            }
            ResourceType::Key => {
                let keys = self.ec2.get_all_key_pairs().await?;
                output.push(
                    r#"<table border="1" class="dataframe">
                        <thead><tr><th>Key Name</th><th>Key Fingerprint</th></tr></thead>
                        <tbody>"#
                        .into(),
                );
                let result: Vec<_> = keys
                    .map(|(key, fingerprint)| {
                        format!(
                            r#"<tr style="text-align: center;"><td>{}</td><td>{}</td></tr>"#,
                            key, fingerprint
                        )
                        .into()
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".into());
            }
            ResourceType::Volume => {
                let volumes: Vec<_> = self.ec2.get_all_volumes().await?.collect();
                if volumes.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead><tr><th></th><th>Volume ID</th>
                    <th>Availability Zone</th><th>Size</th><th>IOPS</th><th>State</th><th>Tags</th>
                    </tr></thead><tbody>"#
                        .into(),
                );
                let result: Vec<_> = volumes
                    .iter()
                    .map(|vol| {
                        let vol_sizes: Vec<_> = get_volumes(vol.size).into_iter().map(|s| {
                            format!(r#"<option value="{s}">{s} GB</option>"#, s = s)
                        }).collect();
                        format!(
                            r#"<tr style="text-align: center;">
                                <td>{}</td><td>{}</td><td>{}</td><td><select id="vol_size">{}</select></td><td>{}</td><td>{}</td>
                                <td>{}</td><td>{}</td></tr>"#,
                            if let Some("ddbolineinthecloud") = vol.tags.get("Name").map(StackString::as_str) {
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
                            vol_sizes.join("\n"),
                            vol.iops,
                            vol.state,
                            if vol.tags.is_empty() {
                                format!(
                                    r#"
                                        <input type="text" name="tag_volume" id="tag_volume">
                                        <input type="button" name="tag_volume" value="Tag" onclick="tagVolume('{}');">
                                    "#, vol.id
                                )
                            } else {
                                print_tags(&vol.tags).into()
                            },
                            if let Some("ddbolineinthecloud") = vol.tags.get("Name").map(StackString::as_str) {
                                format!(
                                    r#"<input type="button" name="CreateSnapshot" value="CreateSnapshot"
                                        onclick="createSnapshot('{}', '{}')">"#,
                                    vol.id,
                                    format!("dileptoninthecloud_backup_{}", Local::now().naive_local().date().format("%Y%m%d")),
                                )
                            } else {
                                format!(
                                    r#"<input type="button" name="ModifyVolume" value="ModifyVolume"
                                        onclick="modifyVolume('{}')">"#,
                                    vol.id
                                )
                            },
                        ).into()
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".into());
            }
            ResourceType::Snapshot => {
                let mut snapshots: Vec<_> = self.ec2.get_all_snapshots().await?.collect();
                if snapshots.is_empty() {
                    return Ok(Vec::new());
                }
                snapshots.sort_by(|x, y| {
                    let x = x.tags.get("Name").map_or("", StackString::as_str);
                    let y = y.tags.get("Name").map_or("", StackString::as_str);
                    x.cmp(&y)
                });
                output.push(
                    r#"<table border="1" class="dataframe"><thead><tr>
                        <th></th><th>Snapshot ID</th><th>Size</th><th>State</th><th>Progress</th>
                        <th>Tags</th></tr></thead><tbody>"#
                        .into(),
                );
                let result: Vec<_> = snapshots
                    .iter()
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
                            if snap.tags.is_empty() {
                                format!(
                                    r#"
                                        <input type="text" name="tag_snapshot" id="tag_snapshot">
                                        <input type="button" name="tag_snapshot" value="Tag" onclick="tagSnapshot('{}');">
                                    "#, snap.id
                                )
                            } else {
                                print_tags(&snap.tags).into()
                            }
                        )
                        .into()
                    })
                    .collect();
                output.extend_from_slice(&result);
                output.push("</tbody></table>".into());
            }
            ResourceType::Ecr => {
                let repos: Vec<_> = self.ecr.get_all_repositories().await?.collect();
                if repos.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    r#"<table border="1" class="dataframe"><thead><tr>
                        <th><input type="button" name="CleanupEcr" value="CleanupEcr"
                            onclick="cleanupEcrImages()"></th>
                            <th>ECR Repo</th><th>Tag</th><th>Digest</th><th>Pushed At</th>
                            <th>Image Size</th></tr></thead><tbody>"#
                        .into(),
                );

                let futures = repos.iter().map(|repo| get_ecr_images(self, &repo));
                let results: Vec<_> = try_join_all(futures)
                    .await?
                    .into_iter()
                    .flatten()
                    .map(Into::into)
                    .collect();
                output.extend_from_slice(&results);
                output.push("</tbody></table>".into());
            }
            ResourceType::Script => {
                output.push(
                    r#"
                        <form action="javascript:createScript()">
                        <input type="text" name="script_filename" id="script_filename"/>
                        <input type="button" name="create_script" value="New"
                            onclick="createScript();"/></form>"#
                        .into(),
                );
                let result: Vec<_> = self
                    .get_all_scripts()?
                    .into_iter()
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
                        .into()
                    })
                    .collect();
                output.extend_from_slice(&result);
            }
            ResourceType::User => {
                let users = self
                    .iam
                    .list_users()
                    .await?
                    .map(|u| {
                        format!(
                            "{} {} {:30} {:60}",
                            u.user_id, u.create_date, u.user_name, u.arn,
                        )
                    })
                    .join("\n");
                output.push(users);
            }
            ResourceType::Group => {
                let groups = self
                    .iam
                    .list_groups()
                    .await?
                    .map(|g| {
                        format!(
                            "{} {} {:30} {:60}",
                            g.group_id, g.create_date, g.group_name, g.arn,
                        )
                    })
                    .join("\n");
                output.push(groups);
            }
            ResourceType::AccessKey => {
                let futures =
                    self.iam.list_users().await?.map(|user| async move {
                        self.iam.list_access_keys(&user.user_name).await
                    });
                let results: Result<Vec<Vec<_>>, Error> = try_join_all(futures).await;
                let keys = results?
                    .into_iter()
                    .map(|keys| {
                        keys.into_iter()
                            .filter_map(|key| {
                                Some(format!(
                                    "{} {:30} {} {}",
                                    key.access_key_id?,
                                    key.user_name?,
                                    key.create_date?,
                                    key.status?
                                ))
                            })
                            .join("\n")
                    })
                    .join("\n");
                output.push(keys);
            }
        };
        Ok(output)
    }
}

async fn list_instance(app: &AwsAppInterface) -> Result<Vec<StackString>, Error> {
    app.fill_instance_list().await?;

    let result: Vec<_> = INSTANCE_LIST
        .read()
        .await
        .iter()
        .map(|inst| {
            let status_button = if &inst.state == "running" {
                format!(
                    r#"<input type="button" name="Status" value="Status" {}>"#,
                    format!(r#"onclick="getStatus('{}')""#, inst.id)
                )
            } else {
                "".to_string()
            };
            let name = inst.tags.get("Name").cloned().unwrap_or_else(|| "".into());
            let name_button = if &inst.state == "running" && &name != "ddbolineinthecloud" {
                format!(
                    r#"<input type="button" name="CreateImage {name}" value="{name}" {button}>"#,
                    name = name,
                    button = format!(
                        r#"onclick="createImage('{inst_id}', '{name}')""#,
                        inst_id = inst.id,
                        name = name
                    )
                )
            } else {
                name.to_string()
            };
            let terminate_button = if &inst.state == "running" && &name != "ddbolineinthecloud" {
                format!(
                    r#"<input type="button" name="Terminate" value="Terminate" {}>"#,
                    format!(r#"onclick="terminateInstance('{}')""#, inst.id)
                )
            } else {
                "".to_string()
            };
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
                name_button,
                inst.instance_type,
                inst.launch_time.with_timezone(&Local),
                inst.availability_zone,
                status_button,
                terminate_button,
            )
            .into()
        })
        .collect();
    Ok(result)
}

async fn get_ecr_images(
    app: &AwsAppInterface,
    repo: &str,
) -> Result<impl Iterator<Item = StackString>, Error> {
    app.ecr
        .get_all_images(&repo)
        .await
        .map_err(Into::into)
        .map(|it| it.map(|image| image.get_html_string()))
}

fn print_tags<T: Display>(tags: &HashMap<T, T>) -> StackString {
    tags.iter()
        .map(|(k, v)| format!("{} = {}", k, v))
        .join(", ")
        .into()
}

#[derive(Serialize, Deserialize)]
pub struct TerminateRequest {
    pub instance: StackString,
}

#[async_trait]
impl HandleRequest<TerminateRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: TerminateRequest) -> Self::Result {
        self.terminate(&[req.instance]).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct CreateImageRequest {
    pub inst_id: StackString,
    pub name: StackString,
}

#[async_trait]
impl HandleRequest<CreateImageRequest> for AwsAppInterface {
    type Result = Result<Option<StackString>, Error>;
    async fn handle(&self, req: CreateImageRequest) -> Self::Result {
        self.create_image(&req.inst_id, &req.name).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteImageRequest {
    pub ami: StackString,
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
    pub volid: StackString,
}

#[async_trait]
impl HandleRequest<DeleteVolumeRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: DeleteVolumeRequest) -> Self::Result {
        self.delete_ebs_volume(&req.volid).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct ModifyVolumeRequest {
    volid: StackString,
    size: i64,
}

#[async_trait]
impl HandleRequest<ModifyVolumeRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: ModifyVolumeRequest) -> Self::Result {
        self.modify_ebs_volume(&req.volid, req.size).await?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteSnapshotRequest {
    pub snapid: StackString,
}

#[async_trait]
impl HandleRequest<DeleteSnapshotRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: DeleteSnapshotRequest) -> Self::Result {
        self.delete_ebs_snapshot(&req.snapid).await
    }
}

#[derive(Serialize, Deserialize)]
pub struct CreateSnapshotRequest {
    pub volid: StackString,
    pub name: Option<StackString>,
}

#[async_trait]
impl HandleRequest<CreateSnapshotRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: CreateSnapshotRequest) -> Self::Result {
        let tags = if let Some(name) = &req.name {
            hashmap! {"Name".into() => name.clone()}
        } else {
            HashMap::new()
        };
        self.create_ebs_snapshot(req.volid.as_str(), &tags)
            .await
            .map(|_| ())
    }
}

#[derive(Serialize, Deserialize)]
pub struct TagItemRequest {
    pub id: StackString,
    pub tag: StackString,
}

#[async_trait]
impl HandleRequest<TagItemRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    async fn handle(&self, req: TagItemRequest) -> Self::Result {
        self.ec2
            .tag_ec2_instance(
                req.id.as_str(),
                &hashmap! {
                    "Name".into() => req.tag,
                },
            )
            .await
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteEcrImageRequest {
    pub reponame: StackString,
    pub imageid: StackString,
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
    pub instance: StackString,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CommandRequest {
    pub instance: StackString,
    pub command: StackString,
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
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?;
                let websockify_command = Command::new("sudo")
                    .args(&[
                        &websockify.to_string_lossy(),
                        "8787",
                        "--ssl-only",
                        "--web",
                        novnc_path.to_string_lossy().as_ref(),
                        "--cert",
                        &cert.to_string_lossy(),
                        "--key",
                        &key.to_string_lossy(),
                        "localhost:5900",
                    ])
                    .kill_on_drop(true)
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
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
    type Result = Result<Vec<StackString>, Error>;
    async fn handle(&self, _: NoVncStopRequest) -> Self::Result {
        let mut children = NOVNC_CHILDREN.write().await;
        for child in children.iter_mut() {
            if let Err(e) = child.kill() {
                debug!("Failed to kill {}", e);
            }
        }

        let mut kill = Command::new("sudo");
        kill.args(&["kill", "-9"]);
        let ids = get_websock_pids().await?.into_iter().map(|x| x.to_string());
        kill.args(ids);
        let kill = kill.spawn()?;
        kill.wait_with_output().await?;

        let mut output = Vec::new();
        while let Some(mut child) = children.pop() {
            if let Err(e) = child.kill() {
                debug!("Failed to kill {}", e);
            }
            let result = child.wait_with_output().await?;
            output.push(StackString::from_utf8(result.stdout)?);
            output.push(StackString::from_utf8(result.stderr)?);
        }
        children.clear();
        Ok(output)
    }
}

pub async fn get_websock_pids() -> Result<Vec<usize>, Error> {
    let websock = Command::new("ps")
        .args(&["-eF"])
        .stdout(Stdio::piped())
        .spawn()?;
    let output = websock.wait_with_output().await?;
    let output = StackString::from_utf8(output.stdout)?;
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

fn get_volumes(current_vol: i64) -> SmallVec<[i64; 8]> {
    [8, 16, 32, 64, 100, 200, 400, 500]
        .iter()
        .map(|x| if *x < current_vol { current_vol } else { *x })
        .dedup()
        .collect()
}
