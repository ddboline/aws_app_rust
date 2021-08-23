use cached::{proc_macro::cached, SizedCache};
use chrono::Local;
use futures::future::try_join_all;
use itertools::Itertools;
use maplit::hashmap;
use rweb::Schema;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use stack_string::StackString;
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
};
use tokio::try_join;

use aws_app_lib::{
    aws_app_interface::{AwsAppInterface, INSTANCE_LIST},
    ec2_instance::AmiInfo,
    resource_type::ResourceType,
    systemd_instance::RunStatus,
};

use crate::errors::ServiceError as Error;

#[cached(
    type = "SizedCache<String, Option<AmiInfo>>",
    create = "{ SizedCache::with_size(10) }",
    convert = r#"{ format!("{}-{}", ubuntu_release, arch) }"#,
    result = true
)]
async fn get_latest_ubuntu_ami(
    app: &AwsAppInterface,
    ubuntu_release: &str,
    arch: &str,
) -> Result<Option<AmiInfo>, Error> {
    app.ec2
        .get_latest_ubuntu_ami(ubuntu_release, arch)
        .await
        .map_err(Into::into)
}

pub async fn get_frontpage(
    resource_type: ResourceType,
    app: &AwsAppInterface,
) -> Result<Vec<StackString>, Error> {
    let mut output: Vec<StackString> = Vec::new();
    match resource_type {
        ResourceType::Instances => {
            let result = list_instance(app).await?;
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
            let reserved: Vec<_> = app.ec2.get_reserved_instances().await?.collect();
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
            let requests: Vec<_> = app.ec2.get_spot_instance_requests().await?.collect();
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
                get_latest_ubuntu_ami(app, &app.config.ubuntu_release, "amd64")
                    .await
                    .map_err(Into::into)
            };
            let ubuntu_ami_arm64 = async {
                get_latest_ubuntu_ami(app, &app.config.ubuntu_release, "arm64")
                    .await
                    .map_err(Into::into)
            };

            let ami_tags = app.ec2.get_ami_tags();
            let (ubuntu_ami, ubuntu_ami_arm64, ami_tags) =
                try_join!(ubuntu_ami, ubuntu_ami_arm64, ami_tags)?;
            let mut ami_tags: Vec<_> = ami_tags.collect();

            ami_tags.sort_by(|x, y| x.name.cmp(&y.name));
            if let Some(ami) = ubuntu_ami {
                ami_tags.push(ami);
            }
            if let Some(ami) = ubuntu_ami_arm64 {
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
            let keys = app.ec2.get_all_key_pairs().await?;
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
            let volumes: Vec<_> = app.ec2.get_all_volumes().await?.collect();
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
                                <td>{}</td><td>{}</td><td>{}</td>
                                <td><select id="{}_vol_size">{}</select></td>
                                <td>{}</td><td>{}</td>
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
                            vol.id,
                            vol_sizes.join("\n"),
                            vol.iops,
                            vol.state,
                            if vol.tags.is_empty() {
                                format!(
                                    r#"
                                        <input type="text" name="tag_volume" id="{}_tag_volume">
                                        <input type="button" name="tag_volume" value="Tag" onclick="tagVolume('{}');">
                                    "#, vol.id, vol.id,
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
            let mut snapshots: Vec<_> = app.ec2.get_all_snapshots().await?.collect();
            if snapshots.is_empty() {
                return Ok(Vec::new());
            }
            snapshots.sort_by(|x, y| {
                let x = x.tags.get("Name").map_or("", StackString::as_str);
                let y = y.tags.get("Name").map_or("", StackString::as_str);
                x.cmp(y)
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
                                        <input type="text" name="tag_snapshot" id="{}_tag_snapshot">
                                        <input type="button" name="tag_snapshot" value="Tag" onclick="tagSnapshot('{}');">
                                    "#, snap.id, snap.id,
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
            let repos: Vec<_> = app.ecr.get_all_repositories().await?.collect();
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

            let futures = repos.iter().map(|repo| get_ecr_images(app, repo));
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
            let result: Vec<_> = app
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
            output.push(
                    r#"<table border="1" class="dataframe"><thead><tr><th>User ID</th><th>Create Date</th>
                    <th>User Name</th><th>Arn</th><th></th><th>Groups</th><th></th>
                    </tr></thead><tbody>"#
                        .into(),
                );
            let _user_name: Option<&str> = None;
            let (current_user, users) =
                try_join!(app.iam.get_user(_user_name), app.iam.list_users())?;
            let users: Vec<_> = users.collect();
            let futures = users.iter().map(|u| async move {
                app.iam
                    .list_groups_for_user(u.user_name.as_str())
                    .await
                    .map(|g| {
                        let groups: Vec<_> = g.collect();
                        (u.user_name.clone(), groups)
                    })
            });
            let results: Result<Vec<_>, Error> = try_join_all(futures).await.map_err(Into::into);
            let group_map: HashMap<StackString, _> = results?.into_iter().collect();

            let futures = users.iter().map(|u| async move {
                app.iam
                    .list_access_keys(u.user_name.as_str())
                    .await
                    .map(|metadata| (u.user_name.clone(), metadata))
            });
            let results: Result<Vec<_>, Error> = try_join_all(futures).await.map_err(Into::into);
            let key_map: HashMap<StackString, _> = results?.into_iter().collect();

            let users = users
                    .into_iter()
                    .map(|u| {
                        let group_select = if let Some(group_opts) =
                            group_map.get(u.user_name.as_str()).map(|x| {
                                x.iter()
                                    .map(|group| {
                                        format!(
                                            r#"r#"<option value="{g}">{g}</option>"#,
                                            g = group.group_name
                                        )
                                    })
                                    .join("")
                            }) {
                            format!(r#"<select id="group_opt">{}</select>"#, group_opts)
                        } else {
                            "".to_string()
                        };
                        let group_remove_button = if group_select.is_empty() {
                            "".to_string()
                        } else {
                            format!(
                                r#"
                                    <input type="button" name="RemoveUser" value="Remove" id="{}_group_opt"
                                     onclick="removeUserFromGroup('{}');">"#,
                                u.user_name, u.user_name,
                            )
                        };
                        let delete_button = if u.user_id == current_user.user_id {
                            "".to_string()
                        } else {
                            format!(
                                r#"<input type="button" name="DeleteUser" value="DeleteUser"
                                onclick="deleteUser('{}')">"#,
                                u.user_name,
                            )
                        };
                        let empty_vec = Vec::new();
                        let access_keys = key_map.get(u.user_name.as_str()).unwrap_or(&empty_vec);
                        let create_key_button = if access_keys.len() < 2 {
                            format!(
                                r#"<input type="button" name="CreateKey" value="CreateKey"
                                onclick="createAccessKey('{}')">"#,
                                u.user_name,
                            )
                        } else {
                            "".to_string()
                        };
                        format!(
                            r#"
                                <tr style="text-align: left;">
                                <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td>
                                <td>{}</td><td>{}</td>
                                </tr>
                            "#,
                            u.user_id,
                            u.create_date,
                            u.user_name,
                            u.arn,
                            delete_button,
                            group_select,
                            group_remove_button,
                            create_key_button,
                        )
                    })
                    .join("");
            output.push(users.into());
            output.push(r#"</tbody></table>"#.into());
        }
        ResourceType::Group => {
            output.push(
                    r#"<table border="1" class="dataframe"><thead><tr><th>Group ID</th><th>Create Date</th>
                    <th>Group Name</th><th>Arn</th><th></th>
                    </tr></thead><tbody>"#
                        .into(),
                );
            let (users, groups) = try_join!(app.iam.list_users(), app.iam.list_groups())?;
            let users: HashSet<_> = users.map(|u| u.user_name).collect();
            let futures = users.iter().map(|u| async move {
                app.iam
                    .list_groups_for_user(u.as_str())
                    .await
                    .map(|g| g.map(|group| (u.clone(), group)).collect::<Vec<_>>())
            });
            let results: Result<Vec<_>, Error> = try_join_all(futures).await.map_err(Into::into);
            let user_map: HashMap<StackString, HashSet<StackString>> = results?
                .into_iter()
                .flatten()
                .fold(HashMap::new(), |mut h, (u, g)| {
                    h.entry(g.group_name).or_default().insert(u);
                    h
                });

            let groups = groups
                .map(|g| {
                    let empty_set = HashSet::new();
                    let group_users = user_map.get(g.group_name.as_str()).unwrap_or(&empty_set);

                    let user_opts = users
                        .iter()
                        .filter_map(|u| {
                            if group_users.contains(u) {
                                None
                            } else {
                                Some(format!(r#"r#"<option value="{u}">{u}</option>"#, u = u))
                            }
                        })
                        .join("");

                    let user_select = if user_opts.is_empty() {
                        "".to_string()
                    } else {
                        format!(
                            r#"<select id="{}_user_opt">{}</select>"#,
                            g.group_name, user_opts
                        )
                    };

                    let user_add_button = if user_select.is_empty() {
                        "".to_string()
                    } else {
                        format!(
                            r#"
                                    <input type="button" name="AddUser" value="Add"
                                     onclick="addUserToGroup('{}');">"#,
                            g.group_name
                        )
                    };

                    format!(
                        r#"
                                <tr style="text-align: left;">
                                <td>{}</td><td>{}</td><td>{}</td><td>{}</td>
                                <td>{}</td><td>{}</td>
                                </tr>
                            "#,
                        g.group_id,
                        g.create_date,
                        g.group_name,
                        g.arn,
                        user_select,
                        user_add_button,
                    )
                })
                .join("");
            output.push(groups.into());
            output.push(r#"</tbody></table>"#.into());
        }
        ResourceType::AccessKey => {
            output.push(
                r#"<table border="1" class="dataframe"><thead><tr><th>Key ID</th><th>User Name</th>
                    <th>Create Date</th><th>Status</th><th></th>
                    </tr></thead><tbody>"#
                    .into(),
            );
            let futures = app
                .iam
                .list_users()
                .await?
                .map(|user| async move { app.iam.list_access_keys(&user.user_name).await });
            let results: Result<Vec<Vec<_>>, Error> =
                try_join_all(futures).await.map_err(Into::into);
            let keys = results?
                    .into_iter()
                    .map(|keys| {
                        keys.into_iter()
                            .filter_map(|key| {
                                let user_name = key.user_name?;
                                let access_key_id = key.access_key_id?;
                                let delete_key_button = format!(
                                    r#"<input type="button" name="DeleteKey" value="Delete"
                                        onclick="deleteAccessKey('{}', '{}');">"#,
                                    user_name, access_key_id
                                );
                                Some(format!(
                                    r#"<tr style="text-align: left;">
                                        <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                                    access_key_id,
                                    user_name,
                                    key.create_date?,
                                    key.status?,
                                    delete_key_button,
                                ))
                            })
                            .join("")
                    })
                    .join("");
            output.push(keys.into());
            output.push(r#"</tbody></table>"#.into());
        }
        ResourceType::Route53 => {
            let current_ip = app.route53.get_ip_address().await?;
            output.push(
                r#"<table border="1" class="dataframe"><thead><tr><th>Zone ID</th><th>DNS Name</th>
                    <th>IP Address</th><th></th>
                    </tr></thead><tbody>"#
                    .into(),
            );
            let records = app.route53.list_all_dns_records().await?.into_iter().map(|(zone, name, ip)| {
                let update_dns_button = format!(
                    r#"<input type="button" name="Update" value="{new_ip}"
                        onclick="updateDnsName('{zone}', '{dns}.', '{old_ip}', '{new_ip}');">"#,
                    zone=zone, dns=name, old_ip=ip, new_ip=current_ip,
                );
                format!(
                    r#"<tr style="text-align; left;"><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                    zone, name, ip, update_dns_button
                )
            }).join("");
            output.push(records.into());
            output.push(r#"</tbody></table>"#.into());
        }
        ResourceType::SystemD => {
            let services = app.systemd.list_running_services().await?;
            output.push(
                r#"<table border="1" class="dataframe"><thead><tr>
                   <th>Name</th><th>Status</th><th>
                   <input type="button" name="RestartAll" value="RestartAll" onclick="systemdRestartAll();">
                   </th><th>
                   <input type="button" name="Crontab" value="Crontab" onclick="crontabLogs('user');"><br>
                   <input type="button" name="CrontabRoot" value="CrontabRoot" onclick="crontabLogs('root');">
                   </th></thead>"#.into()
            );
            let records = app.config.systemd_services.iter().map(|service| {
                let log_button = format!(
                    r#"<input type="button" name="SystemdLogs" value="Logs" onclick="systemdLogs('{service}');">"#,
                    service=service,
                );
                let run_status = services.get(service).unwrap_or(&RunStatus::NotRunning);
                let action_button = match run_status {
                    RunStatus::Running => {
                        format!(
                            r#"<input type="button" name="SystemdRestart" value="Restart" onclick="systemdAction('restart', '{service}');">
                               <input type="button" name="SystemdStop" value="Stop" onclick="systemdAction('stop', '{service}');">"#,
                            service=service,
                        )
                    },
                    RunStatus::NotRunning => {
                        format!(
                            r#"<input type="button" name="SystemdStart" value="Start" onclick="systemdAction('start', '{service}');">"#,
                            service=service
                        )
                    },
                };
                format!(
                    r#"<tr style=text-align; left;"><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                    service, run_status, action_button, log_button
                )
            }).join("\n");
            output.push(records.into());
            output.push(r#"</tbody></table>"#.into());
        }
    };
    Ok(output)
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
            let empty: StackString = "".into();
            let name = inst.tags.get("Name").unwrap_or(&empty);
            let name_button = if &inst.state == "running" && name != "ddbolineinthecloud" {
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
            let terminate_button = if &inst.state == "running" && name != "ddbolineinthecloud" {
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
        .get_all_images(repo)
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

#[derive(Serialize, Deserialize, Schema)]
pub struct TerminateRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CreateImageRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub inst_id: StackString,
    #[schema(description = "Ami Name")]
    pub name: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteImageRequest {
    #[schema(description = "Ami ID")]
    pub ami: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteVolumeRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ModifyVolumeRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
    #[schema(description = "Volume Size GiB")]
    pub size: i64,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteSnapshotRequest {
    #[schema(description = "Snapshot ID")]
    pub snapid: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct CreateSnapshotRequest {
    #[schema(description = "Volume ID")]
    pub volid: StackString,
    #[schema(description = "Snapshot Name")]
    pub name: Option<StackString>,
}

impl CreateSnapshotRequest {
    pub async fn handle(&self, app: &AwsAppInterface) -> Result<(), Error> {
        let tags = if let Some(name) = &self.name {
            hashmap! {"Name".into() => name.clone()}
        } else {
            HashMap::new()
        };
        app.create_ebs_snapshot(self.volid.as_str(), &tags)
            .await
            .map(|_| ())
            .map_err(Into::into)
    }
}

#[derive(Serialize, Deserialize, Schema)]
pub struct TagItemRequest {
    #[schema(description = "Resource ID")]
    pub id: StackString,
    #[schema(description = "Tag")]
    pub tag: StackString,
}

impl TagItemRequest {
    pub async fn handle(self, app: &AwsAppInterface) -> Result<(), Error> {
        app.ec2
            .tag_ec2_instance(
                self.id.as_str(),
                &hashmap! {
                    "Name".into() => self.tag,
                },
            )
            .await
            .map_err(Into::into)
    }
}

#[derive(Serialize, Deserialize, Schema)]
pub struct DeleteEcrImageRequest {
    #[schema(description = "ECR Repository Name")]
    pub reponame: StackString,
    #[schema(description = "Container Image ID")]
    pub imageid: StackString,
}

#[derive(Serialize, Deserialize, Schema)]
pub struct StatusRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
}

#[derive(Serialize, Deserialize, Debug, Schema)]
pub struct CommandRequest {
    #[schema(description = "Instance ID or Name Tag")]
    pub instance: StackString,
    #[schema(description = "Command String")]
    pub command: StackString,
}

fn get_volumes(current_vol: i64) -> SmallVec<[i64; 8]> {
    [8, 16, 32, 64, 100, 200, 400, 500]
        .iter()
        .map(|x| if *x < current_vol { current_vol } else { *x })
        .dedup()
        .collect()
}
