use chrono::Local;
use failure::Error;
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use rayon::slice::ParallelSliceMut;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use aws_app_lib::aws_app_interface::{AwsAppInterface, INSTANCE_LIST};
use aws_app_lib::resource_type::ResourceType;

pub trait HandleRequest<T> {
    type Result;
    fn handle(&self, req: T) -> Self::Result;
}

impl HandleRequest<ResourceType> for AwsAppInterface {
    type Result = Result<Vec<String>, Error>;
    fn handle(&self, req: ResourceType) -> Self::Result {
        let mut output = Vec::new();
        match req {
            ResourceType::Instances => {
                self.fill_instance_list()?;

                let result: Vec<_> = INSTANCE_LIST
                    .read()
                    .par_iter()
                    .map(|inst| {
                        let name = inst
                            .tags
                            .get("Name")
                            .cloned()
                            .unwrap_or_else(|| "".to_string());
                        format!(
                            "{} {} {} {} {} {} {} {}",
                            inst.id,
                            inst.dns_name,
                            inst.state,
                            name,
                            inst.instance_type,
                            inst.launch_time.with_timezone(&Local),
                            inst.availability_zone,
                            if inst.state == "running" && name != "ddbolineinthecloud" {
                                format!(
                                    r#"<input type="button" name="Terminate" value="Terminate" {}>"#,
                                    format!(r#"onclick="terminateInstance('{}')""#, inst.id)
                                )
                            } else {"".to_string()}
                        )
                    })
                    .collect();
                if !result.is_empty() {
                    output.push("instances:".to_string());
                    output.extend_from_slice(&result);
                }
            }
            ResourceType::Reserved => {
                let reserved = self.ec2.get_reserved_instances()?;
                if reserved.is_empty() {
                    return Ok(Vec::new());
                }
                output.push("Get Reserved Instance".to_string());
                let result: Vec<_> = reserved
                    .par_iter()
                    .map(|res| {
                        format!(
                            "{} {} {} {} {}",
                            res.id, res.price, res.instance_type, res.state, res.availability_zone
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
            }
            ResourceType::Spot => {
                let requests = self.ec2.get_spot_instance_requests()?;
                if requests.is_empty() {
                    return Ok(Vec::new());
                }
                output.push("Spot Instance Requests:".to_string());
                let result: Vec<_> = requests
                    .par_iter()
                    .map(|req| {
                        format!(
                            "{} {} {} {} {} {}",
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
            }
            ResourceType::Ami => {
                let mut ami_tags = self.ec2.get_ami_tags()?;
                if ami_tags.is_empty() {
                    return Ok(Vec::new());
                }
                let mut ubuntu_amis = self
                    .ec2
                    .get_latest_ubuntu_ami(&self.config.ubuntu_release)?;
                ubuntu_amis.par_sort_by_key(|x| x.name.clone());
                if !ubuntu_amis.is_empty() {
                    ami_tags.push(ubuntu_amis[ubuntu_amis.len() - 1].clone());
                }
                output.push("AMI's:".to_string());
                let result: Vec<_> = ami_tags
                    .par_iter()
                    .map(|ami| {
                        format!(
                            "{} {} {} {} {}",
                            format!(
                                r#"<input type="button" name="DeleteImage" value="DeleteImage" onclick="deleteImage('{}')">"#,
                                ami.id
                            ),
                            ami.id,
                            ami.name,
                            ami.state,
                            ami.snapshot_ids.join(" ")
                        )
                    })
                    .collect();
                output.extend_from_slice(&result);
            }
            ResourceType::Key => {
                let keys = self.ec2.get_all_key_pairs()?;
                output.push("Keys:".to_string());
                let result: Vec<_> = keys
                    .into_par_iter()
                    .map(|(key, fingerprint)| format!("{} {}", key, fingerprint))
                    .collect();
                output.extend_from_slice(&result);
            }
            ResourceType::Volume => {
                let volumes = self.ec2.get_all_volumes()?;
                if volumes.is_empty() {
                    return Ok(Vec::new());
                }
                output.push("Volumes:".to_string());
                let result: Vec<_> = volumes
                    .par_iter()
                    .map(|vol| {
                        format!(
                            "{} {} {} {} {} {} {}",
                            format!(
                                r#"<input type="button" name="DeleteVolume" value="DeleteVolume" onclick="deleteVolume('{}')">"#,
                                vol.id
                            ),
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
            }
            ResourceType::Snapshot => {
                let snapshots = self.ec2.get_all_snapshots()?;
                if snapshots.is_empty() {
                    return Ok(Vec::new());
                }
                output.push("Snapshots:".to_string());
                let result: Vec<_> = snapshots
                    .par_iter()
                    .map(|snap| {
                        format!(
                            "{} {} {} GB {} {} {}",
                            format!(
                                r#"<input type="button" name="DeleteSnapshot" value="DeleteSnapshot" onclick="deleteSnapshot('{}')">"#,
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
            }
            ResourceType::Ecr => {
                let repos = self.ecr.get_all_repositories()?;
                if repos.is_empty() {
                    return Ok(Vec::new());
                }
                output.push(
                    format!(
                        "ECR images {}",
                        r#"<input type="button" name="CleanupEcr" value="CleanupEcr" onclick="cleanupEcrImages()">"#,
                    )
                );
                let result: Result<Vec<_>, Error> = repos
                    .par_iter()
                    .map(|repo| {
                        let images = self.ecr.get_all_images(&repo)?;
                        let lines: Vec<_> = images
                            .par_iter()
                            .map(|image| {
                                format!(
                                    "{} {} {} {}",
                                    format!(
                                        r#"<input type="button" name="DeleteEcrImage" value="DeleteEcrImage" onclick="deleteEcrImage('{}', '{}')">"#,
                                        repo, image.digest,
                                    ),
                                    repo,
                                    image
                                        .tags
                                        .get(0)
                                        .map_or_else(|| "None", String::as_str),
                                    image.digest
                                )
                            })
                            .collect();
                        Ok(lines)
                    })
                    .collect();
                let result: Vec<_> = result?.into_par_iter().flatten().collect();
                output.extend_from_slice(&result);
            }
        };
        Ok(output)
    }
}

fn print_tags(tags: &HashMap<String, String>) -> String {
    let results: Vec<_> = tags
        .par_iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect();
    results.join(", ")
}

#[derive(Serialize, Deserialize)]
pub struct TerminateRequest {
    pub instance: String,
}

impl HandleRequest<TerminateRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    fn handle(&self, req: TerminateRequest) -> Self::Result {
        self.terminate(&[req.instance])
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteImageRequest {
    pub ami: String,
}

impl HandleRequest<DeleteImageRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    fn handle(&self, req: DeleteImageRequest) -> Self::Result {
        self.delete_image(&req.ami)
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteVolumeRequest {
    pub volid: String,
}

impl HandleRequest<DeleteVolumeRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    fn handle(&self, req: DeleteVolumeRequest) -> Self::Result {
        self.delete_ebs_volume(&req.volid)
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteSnapshotRequest {
    pub snapid: String,
}

impl HandleRequest<DeleteSnapshotRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    fn handle(&self, req: DeleteSnapshotRequest) -> Self::Result {
        self.delete_ebs_snapshot(&req.snapid)
    }
}

#[derive(Serialize, Deserialize)]
pub struct DeleteEcrImageRequest {
    pub reponame: String,
    pub imageid: String,
}

impl HandleRequest<DeleteEcrImageRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    fn handle(&self, req: DeleteEcrImageRequest) -> Self::Result {
        self.ecr.delete_ecr_images(&req.reponame, &[req.imageid])
    }
}

pub struct CleanupEcrImagesRequest {}

impl HandleRequest<CleanupEcrImagesRequest> for AwsAppInterface {
    type Result = Result<(), Error>;
    fn handle(&self, _: CleanupEcrImagesRequest) -> Self::Result {
        self.ecr.cleanup_ecr_images()
    }
}
