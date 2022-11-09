use dioxus::prelude::{
    dioxus_elements, fc_to_builder, format_args_f, inline_props, rsx, Element, LazyNodes,
    NodeFactory, Props, Scope, VNode, VirtualDom,
};
use futures::{future::try_join_all, stream::FuturesUnordered, try_join, TryStreamExt};
use stack_string::{format_sstr, StackString};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    net::Ipv4Addr,
    sync::Arc,
};
use time::{macros::format_description, OffsetDateTime};
use time_tz::OffsetDateTimeExt;

use aws_app_lib::{
    aws_app_interface::{AwsAppInterface, AwsInstancePrice, INSTANCE_LIST},
    config::Config,
    date_time_wrapper::DateTimeWrapper,
    ec2_instance::{
        AmiInfo, Ec2InstanceInfo, ReservedInstanceInfo, SnapshotInfo, SpotInstanceRequestInfo,
        VolumeInfo,
    },
    ecr_instance::ImageInfo,
    iam_instance::{AccessKeyMetadata, IamGroup, IamUser},
    models::{InstanceFamily, InstanceList},
    resource_type::ResourceType,
    sysinfo_instance::ProcessInfo,
    systemd_instance::RunStatus,
};

use crate::{
    errors::ServiceError as Error,
    requests::{get_ami_tags, get_volumes, print_tags},
};

/// # Errors
/// Returns error if db query fails
pub async fn get_index(app: &AwsAppInterface) -> Result<StackString, Error> {
    app.fill_instance_list().await?;
    let instances = INSTANCE_LIST.read().await.clone();
    let body = {
        let mut app = VirtualDom::new_with_props(index_list_element, InstanceProps { instances });
        app.rebuild();
        dioxus::ssr::render_vdom(&app)
    };
    Ok(body.into())
}

/// # Errors
/// Returns error if db query fails
pub async fn get_frontpage(
    resource_type: ResourceType,
    aws: &AwsAppInterface,
) -> Result<StackString, Error> {
    let body = match resource_type {
        ResourceType::Instances | ResourceType::All => {
            aws.fill_instance_list().await?;
            let instances = INSTANCE_LIST.read().await.clone();
            let mut app =
                VirtualDom::new_with_props(list_instance_element, InstanceProps { instances });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Reserved => {
            let reserved: Vec<_> = aws.ec2.get_reserved_instances().await?.collect();
            if reserved.is_empty() {
                return Ok(StackString::new());
            }
            let mut app =
                VirtualDom::new_with_props(reserved_element, reserved_elementProps { reserved });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Spot => {
            let requests: Vec<_> = aws.ec2.get_spot_instance_requests().await?.collect();
            if requests.is_empty() {
                return Ok(StackString::new());
            }
            let mut app = VirtualDom::new_with_props(spot_element, spot_elementProps { requests });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Ami => {
            let ami_tags = get_ami_tags(aws).await?;
            let mut app = VirtualDom::new_with_props(ami_element, ami_elementProps { ami_tags });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Key => {
            let keys: Vec<_> = aws.ec2.get_all_key_pairs().await?.collect();
            let mut app = VirtualDom::new_with_props(key_element, key_elementProps { keys });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Volume => {
            let volumes: Vec<_> = aws.ec2.get_all_volumes().await?.collect();
            let mut app =
                VirtualDom::new_with_props(volume_element, volume_elementProps { volumes });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Snapshot => {
            let mut snapshots: Vec<_> = aws.ec2.get_all_snapshots().await?.collect();
            if snapshots.is_empty() {
                return Ok(StackString::new());
            }
            snapshots.sort_by(|x, y| {
                let x = x.tags.get("Name").map_or("", StackString::as_str);
                let y = y.tags.get("Name").map_or("", StackString::as_str);
                x.cmp(y)
            });
            let mut app =
                VirtualDom::new_with_props(snapshot_element, snapshot_elementProps { snapshots });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Ecr => {
            let futures = aws
                .ecr
                .get_all_repositories()
                .await?
                .map(|repo| async move {
                    let images: Vec<_> = aws.ecr.get_all_images(repo).await?.collect();
                    Ok(images)
                });
            let results: Result<Vec<Vec<ImageInfo>>, Error> = try_join_all(futures).await;
            let images: Vec<ImageInfo> = results?.into_iter().flatten().collect();
            if images.is_empty() {
                return Ok(StackString::new());
            }
            let mut app = VirtualDom::new_with_props(ecr_element, ecr_elementProps { images });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Script => {
            let scripts = aws.get_all_scripts();
            if scripts.is_empty() {
                return Ok(StackString::new());
            }
            let mut app =
                VirtualDom::new_with_props(script_element, script_elementProps { scripts });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::User => {
            let user_name: Option<&str> = None;
            let (current_user, users) =
                try_join!(aws.iam.get_user(user_name), aws.iam.list_users())?;
            let users: Vec<_> = users.collect();
            let futures: FuturesUnordered<_> = users
                .iter()
                .map(|u| async move {
                    aws.iam
                        .list_groups_for_user(u.user_name.as_str())
                        .await
                        .map(|g| {
                            let groups: Vec<_> = g.collect();
                            (u.user_name.clone(), groups)
                        })
                })
                .collect();
            let group_map: HashMap<StackString, _> = futures.try_collect().await?;

            let futures: FuturesUnordered<_> = users
                .iter()
                .map(|u| async move {
                    aws.iam
                        .list_access_keys(u.user_name.as_str())
                        .await
                        .map(|metadata| (u.user_name.clone(), metadata))
                })
                .collect();
            let key_map: HashMap<StackString, _> = futures.try_collect().await?;
            let mut app = VirtualDom::new_with_props(
                users_element,
                users_elementProps {
                    users,
                    current_user,
                    group_map,
                    key_map,
                },
            );
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Group => {
            let (users, groups) = try_join!(aws.iam.list_users(), aws.iam.list_groups())?;
            let users: HashSet<_> = users.map(|u| u.user_name).collect();
            let futures = users.iter().map(|u| async move {
                aws.iam
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
            let groups: Vec<_> = groups.collect();
            let mut app = VirtualDom::new_with_props(
                groups_element,
                groups_elementProps {
                    groups,
                    user_map,
                    users,
                },
            );
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::AccessKey => {
            let futures = aws
                .iam
                .list_users()
                .await?
                .map(|user| async move { aws.iam.list_access_keys(user.user_name).await });
            let results: Result<Vec<Vec<_>>, Error> =
                try_join_all(futures).await.map_err(Into::into);
            let keys: Vec<AccessKeyMetadata> = results?.into_iter().flatten().collect();
            let mut app =
                VirtualDom::new_with_props(access_key_element, access_key_elementProps { keys });
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::Route53 => {
            let current_ip = aws.route53.get_ip_address().await?;
            let records = aws.route53.list_all_dns_records().await?;
            let mut app = VirtualDom::new_with_props(
                dns_record_element,
                dns_record_elementProps {
                    records,
                    current_ip,
                },
            );
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
        ResourceType::SystemD => {
            let processes: HashMap<StackString, Vec<_>> = aws
                .sysinfo
                .get_process_info()
                .into_iter()
                .fold(HashMap::new(), |mut h, proc| {
                    h.entry(proc.name.clone()).or_default().push(proc);
                    h
                });
            let services = aws.systemd.list_running_services().await?;
            let config = aws.config.clone();
            let mut app = VirtualDom::new_with_props(
                systemd_element,
                systemd_elementProps {
                    processes,
                    services,
                    config,
                },
            );
            app.rebuild();
            dioxus::ssr::render_vdom(&app)
        }
    };
    Ok(body.into())
}

#[inline_props]
fn index_element<'a>(cx: Scope<'a>, children: Element<'a>) -> Element<'a> {
    cx.render(rsx! {
        head {
            style {[include_str!("../../templates/style.css")]},
        },
        body {
            input {"type": "button", name: "list_inst", value: "Instances", "onclick": "listResource('instances')"},
            input {"type": "button", name: "list_ami", value: "AMIs", "onclick": "listResource('ami');"},
            input {"type": "button", name: "list_vol", value: "Volumes", "onclick": "listResource('volume');"},
            input {"type": "button", name: "list_snap", value: "Snapshots", "onclick": "listResource('snapshot');"},
            input {"type": "button", name: "list_ecr", value: "EcrImages", "onclick": "listResource('ecr');"},
            input {"type": "button", name: "list_key", value: "Keys", "onclick": "listResource('key');"},
            input {"type": "button", name: "list_reserved", value: "ReservedInstances", "onclick": "listResource('reserved');"},
            input {"type": "button", name: "list_requests", value: "SpotRequests", "onclick": "listResource('spot');"},
            input {"type": "button", name: "list_scripts", value: "Scripts", "onclick": "listResource('script');"},
            br {
            input {"type": "button", name: "list_users", value: "Users", "onclick": "listResource('user');"},
            input {"type": "button", name: "list_groups", value: "Groups", "onclick": "listResource('group');"},
            input {"type": "button", name: "list_access_keys", value: "AccessKey", "onclick": "listResource('access-key');"},
            input {"type": "button", name: "list_route53", value: "DnsRecords", "onclick": "listResource('route53');"},
            input {"type": "button", name: "list_systemd", value: "SystemD", "onclick": "listResource('systemd');"},
            input {"type": "button", name: "list_price", value: "Price", "onclick": "listAllPrices()"},
            input {"type": "button", name: "novnc", value: "NoVNC", "onclick": "noVncTab('/aws/novnc/status', 'GET')"},
            input {"type": "button", name: "update", value: "Update", "onclick": "updateMetadata()"},
            button {name: "garminconnectoutput", id: "garminconnectoutput", "&nbsp"},
            },
        },
        article {id: "main_article", children},
        article {id: "sub_article", "&nbsp"},
        script {"language": "Javascript", "type": "text/javascript", [include_str!("../../templates/scripts.js")]},
    })
}

struct InstanceProps {
    instances: Arc<Vec<Ec2InstanceInfo>>,
}

fn index_list_element(cx: Scope<InstanceProps>) -> Element {
    cx.render(rsx! {
        index_element(
            children: crate::elements::list_instance_element(cx)
        )
    })
}

fn list_instance_element(cx: Scope<InstanceProps>) -> Element {
    let local_tz = DateTimeWrapper::local_tz();
    let empty: StackString = "".into();
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Instance Id"},
                    th {"Public Hostname"},
                    th {"State"},
                    th {"Name"},
                    th {"Instance Type"},
                    th {"Created At"},
                    th {"Availability Zone"},
                }
            },
            tbody {
                cx.props.instances.iter().enumerate().map(|(idx, inst)| {
                    let inst_id = &inst.id;
                    let status_button = if &inst.state == "running" {
                        Some(rsx! {
                            input {
                                "type": "button",
                                name: "status",
                                value: "Status",
                                "onclick": "getStatus('{inst_id}')",
                            }
                        })
                    } else {None};
                    let name = inst.tags.get("Name").unwrap_or(&empty);
                    let name_button = if &inst.state == "running" && name != "ddbolineinthecloud" {
                        rsx! {
                            input {
                                "type": "button",
                                name: "CreateImage {name}",
                                value: "{name}",
                                "onclick": "createImage('{inst_id}', '{name}')",
                            }
                        }
                    } else {
                        rsx! {"{name}"}
                    };
                    let terminate_button = if &inst.state == "running" && name != "ddbolineinthecloud" {
                        Some(rsx! {
                            input {
                                "type": "button",
                                name: "Terminate",
                                value: "Terminate",
                                "onclick": "terminateInstance('{inst_id}')",
                            }
                        })
                    } else {None};
                    let dn = &inst.dns_name;
                    let st = &inst.state;
                    let it = &inst.instance_type;
                    let lt = inst.launch_time.to_timezone(local_tz);
                    let az = &inst.availability_zone;
                    rsx! {
                        tr {
                            key: "instance-list-key-{idx}",
                            style: "text-align: center;",
                            td {"{inst_id}"},
                            td {"{dn}"},
                            td {"{st}"},
                            td {name_button},
                            td {"{it}"},
                            td {"{lt}"},
                            td {"{az}"},
                            td {status_button},
                            td {terminate_button},
                        }
                    }
                })
            }
        }
    })
}

#[inline_props]
fn reserved_element(cx: Scope, reserved: Vec<ReservedInstanceInfo>) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Reserved Instance Id"},
                    th {"Price"},
                    th {"Instance Type"},
                    th {"State"},
                    th {"Availability Zone"},
                }
            },
            tbody {
                reserved.iter().enumerate().map(|(idx, res)| {
                    let id = &res.id;
                    let price = res.price;
                    let instance_type = &res.instance_type;
                    let state = &res.state;
                    let ad = res.availability_zone
                        .as_ref()
                        .map_or_else(|| "", StackString::as_str);
                    rsx! {
                        tr {
                            key: "reserved-key-{idx}",
                            "style": "text-align: center;",
                            td {"{id}"},
                            td {"{price}"},
                            td {"{instance_type}"},
                            td {"{state}"},
                            td {"{ad}"},
                        }
                    }
                })
            }
        }
    })
}

#[inline_props]
fn spot_element(cx: Scope, requests: Vec<SpotInstanceRequestInfo>) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Spot Request Id"},
                    th {"Price"},
                    th {"AMI"},
                    th {"Instance Type"},
                    th {"Spot Type"},
                    th {"Status"},
                }
            }
            tbody {
                requests.iter().enumerate().map(|(idx, req)| {
                    let id = &req.id;
                    let pr = req.price;
                    let im = &req.imageid;
                    let it = &req.instance_type;
                    let st = &req.spot_type;
                    let s = &req.status;
                    let pf = match req.status.as_str() {
                        "pending" | "pending-fulfillment" => Some(rsx! {
                            input {
                                "type": "button",
                                name: "cancel",
                                value: "Cancel",
                                "onclick": "cancelSpotRequest('{id}')",
                            }
                        }),
                        _ => None,
                    };
                    rsx! {
                        tr {
                            key: "requests-key-{idx}",
                            style: "text-align: center;",
                            td {"{id}"},
                            td {"${pr}"},
                            td {"{im}"},
                            td {"{it}"},
                            td {"{st}"},
                            td {"{s}"},
                            td {pf},
                        }
                    }
                })
            }
       }
    })
}

#[inline_props]
fn ami_element(cx: Scope, ami_tags: Vec<AmiInfo>) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {},
                    th {},
                    th {"AMI"},
                    th {"Name"},
                    th {"State"},
                    th {"Snapshot ID"},
                },
            },
            tbody {
                ami_tags.iter().enumerate().map(|(idx, ami)| {
                    let id = &ami.id;
                    let nm = &ami.name;
                    let st = &ami.state;
                    let sn = ami.snapshot_ids.join(" ");
                    rsx! {
                        tr {
                            key: "ami-tags-key-{idx}",
                            style: "text-align: center;",
                            td {
                                input {
                                    "type": "button",
                                    name: "DeleteImage",
                                    value: "DeleteImage",
                                    "onclick": "deleteImage('{id}')",
                                }
                            },
                            td {
                                input {
                                    "type": "button",
                                    name: "Request",
                                    value: "Request",
                                    "onclick": "buildSpotRequest('{id}', null, null)",
                                }
                            },
                            td {"{id}"},
                            td {"{nm}"},
                            td {"{st}"},
                            td {"{sn}"},
                        }
                    }
                })
            }
        }
    })
}

#[inline_props]
fn key_element(cx: Scope, keys: Vec<(StackString, StackString)>) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Key Name"}
                    th {"Key Fingerprint"},
                }
           },
           tbody {
            keys.iter().enumerate().map(|(idx, (key, fingerprint))| {
                rsx! {
                    tr {
                        key: "key-{idx}",
                        style: "text-align: center;",
                        td {"{key}"},
                        td {"{fingerprint}"},
                    }
                }
            })
           }
        }
    })
}

#[inline_props]
fn volume_element(cx: Scope, volumes: Vec<VolumeInfo>) -> Element {
    let local_tz = DateTimeWrapper::local_tz();
    cx.render(
        rsx! {
            table {
                "border": "1",
                class: "dataframe",
                thead {
                    tr {
                        th {},
                        th {"Volume ID"},
                        th {"Availability Zone"},
                        th {"Size"},
                        th {"IOPS"},
                        th {"State"},
                        th {"Tags"},
                    }
                }
                tbody {
                    volumes.iter().enumerate().map(|(idx, vol)| {
                        let vs = get_volumes(vol.size).into_iter().enumerate().map(|(i, s)| {
                            rsx! {
                                option {
                                    key: "vs-key-{i}",
                                    value: "{s}",
                                    "{s} GB"
                                }
                            }
                        });
                        let id = &vol.id;
                        let az = &vol.availability_zone;
                        let io = vol.iops;
                        let st = &vol.state;
                        let bt = if vol.tags.get("Name").map(StackString::as_str) == Some("ddbolineinthecloud") {
                            None
                        } else {
                            Some(rsx! {
                                input {
                                    "type": "button",
                                    name: "DeleteVolume",
                                    value: "DeleteVolume",
                                    "onclick": "deleteVolume('{id}')",
                                }
                            })
                        };
                        let tg = if vol.tags.is_empty() {
                            rsx! {
                                input {
                                    "type": "text", name: "tag_volume", id: "{id}_tag_volume",
                                },
                                input {
                                    "type": "button", name: "tag_volume", value: "Tag", "onclick": "tagVolume('{id}');",
                                }
                            }
                        } else {
                            let tags = print_tags(&vol.tags);
                            rsx! {
                                "{tags}"
                            }
                        };
                        let sp = if let Some("ddbolineinthecloud") = vol.tags.get("Name").map(StackString::as_str) {
                            let ymd = format_description!("[year][month][day]");
                            let local = OffsetDateTime::now_utc().to_timezone(local_tz);
                            let local = local.date().format(ymd).unwrap_or_else(|_| String::new());
                            let dt = format_sstr!("dileptoninthecloud_backup_{local}");
                            Some(rsx! {
                                input {
                                    "type": "button", name: "CreateSnapshot", value: "CreateSnapshot",
                                    "onclick": "createSnapshot('{id}', '{dt}')"
                                }
                            })
                        } else {
                            Some(rsx! {
                                input {
                                    "type": "button", name: "ModifyVolume", value: "ModifyVolume",
                                    "onclick": "modifyVolume('{id}')",
                                }
                            })
                        };
                        rsx! {
                            tr {
                                key: "volumes-key-{idx}",
                                style: "text-align: center;",
                                td {bt},
                                td {"{id}"},
                                td {"{az}"},
                                td {
                                    select {
                                        id: "{id}_vol_size",
                                        vs,
                                    }
                                },
                                td {"{io}"},
                                td {"{st}"},
                                td {tg},
                                td {sp},
                            }
                        }
                    })
                }
            }
        }
    )
}

#[inline_props]
fn snapshot_element(cx: Scope, snapshots: Vec<SnapshotInfo>) -> Element {
    cx.render(
        rsx! {
            table {
                "border": "1",
                class: "dataframe",
                thead {
                    tr {
                        th {},
                        th {"Snapshot ID"},
                        th {"Size"},
                        th {"State"},
                        th {"Progress"},
                        th {"Tags"},
                    }
                },
                tbody {
                    snapshots.iter().enumerate().map(|(idx, snap)| {
                        let id = &snap.id;
                        let vs = snap.volume_size;
                        let st = &snap.state;
                        let pr = &snap.progress;
                        let tg = if snap.tags.is_empty() {
                            rsx! {
                                input {
                                    "type": "text", name: "tag_snapshot", id: "{id}_tag_snapshot"
                                }
                                input {
                                    "type": "button", name: "tag_snapshot", value: "Tag", "onclick": "tagSnapshot('{id}');",
                                }
                            }
                        } else {
                            let tags = print_tags(&snap.tags);
                            rsx! {"{tags}"}
                        };
                        rsx! {
                            tr {
                                key: "snapshot-key-{idx}",
                                style: "text-align: center;",
                                td {
                                    input {
                                        "type": "button", name: "DeleteSnapshot", value: "DeleteSnapshot", "onclick": "deleteSnapshot('{id}')",
                                    }
                                },
                                td {"{id}"}
                                td {"{vs} GB"}
                                td {"{st}"}
                                td {"{pr}"}
                                td {tg},
                            }
                        }
                    })
                }
            }
        }
    )
}

#[inline_props]
fn ecr_element(cx: Scope, images: Vec<ImageInfo>) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {
                        input {"type": "button", name: "CleanupEcr", value: "CleanupEcr", "onclick": "cleanupEcrImages()"}
                    },
                    th {"ECR Repo"}, 
                    th {"Tag"},
                    th {"Digest"},
                    th {"Pushed At"},
                    th {"Image Size"},
                }
            },
            tbody {
                images.iter().enumerate().map(|(idx, image)| {
                    let repo = &image.repo;
                    let digest = &image.digest;
                    let tag = image.tags.get(0).map_or_else(|| "None", StackString::as_str);
                    let pushed_at = image.pushed_at;
                    let image_size = image.image_size;
                    rsx! {
                        tr {
                            key: "images-key-{idx}",
                            style: "text-align: center;",
                            td {
                                input {
                                    "type": "button",
                                    name: "DeleteEcrImage",
                                    value: "DeleteEcrImage",
                                    "onclick": "deleteEcrImage('{repo}', '{digest}')",
                                }
                            },
                            td {"{repo}"},
                            td {"{tag}"},
                            td {"{digest}"},
                            td {"{pushed_at}"},
                            td {"{image_size}"},
                        }
                    }
                })
            }
        }
    })
}

#[inline_props]
fn script_element(cx: Scope, scripts: Vec<StackString>) -> Element {
    cx.render(rsx! {
        form {
            action: "javascript:createScript()",
            input {"type": "text", name: "script_filename", id: "script_filename"},
            input {"type": "button", name: "create_script", value: "New", "onclick": "createScript();"}
        }
        scripts.iter().enumerate().map(|(idx, fname)| {
            rsx! {
                div {
                    key: "script-key-{idx}",
                    input {
                        "type": "button", name: "Edit", value: "Edit", "onclick": "editScript('{fname}')",
                    },
                    input {
                        "type": "button", name: "Rm", value: "Rm", "onclick": "deleteScript('{fname}')",
                    },
                    input {
                        "type": "button", name: "Request", value: "Request", "onclick": "buildSpotRequest(null, null, '{fname}')",
                    },
                    " {fname} <br>",
                }
            }
        })
    })
}

#[inline_props]
fn users_element(
    cx: Scope,
    users: Vec<IamUser>,
    current_user: IamUser,
    group_map: HashMap<StackString, Vec<IamGroup>>,
    key_map: HashMap<StackString, Vec<AccessKeyMetadata>>,
) -> Element {
    let empty_vec: Vec<AccessKeyMetadata> = Vec::new();
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"User ID"},
                    th {"Create Date"},
                    th {"User Name"},
                    th {"Arn"},
                    th {},
                    th {"Groups"},
                    th {},
                }
            },
            tbody {
                users.iter().enumerate().map(|(idx, u)| {
                    let user_name = &u.user_name;
                    let group_select = group_map.get(u.user_name.as_str()).map(|x| {
                        rsx! {
                            select {
                                id: "group_opt",
                                x.iter().enumerate().map(|(i, group)| {
                                    let g = &group.group_name;
                                    rsx! {
                                        option {
                                            key: "group-key-{i}",
                                            value: "{g}",
                                            "{g}",
                                        }
                                    }
                                })
                            }
                        }
                    });
                    let group_remove_button = if group_select.is_none() {
                        None
                    } else {
                        Some(rsx! {
                            input {
                                "type": "button",
                                name: "RemoveUser",
                                value: "Remove",
                                id: "{user_name}_group_opt",
                                "onclick": "removeUserFromGroup('{user_name}');",
                            }
                        })
                    };
                    let delete_button = if u.user_id == current_user.user_id {
                        None
                    } else {
                        Some(rsx! {
                            input {
                                "type": "button", name: "DeleteUser", value: "DeleteUser",
                                "onclick": "deleteUser('{user_name}')",
                            }
                        })
                    };
                    let access_keys = key_map.get(u.user_name.as_str()).unwrap_or(&empty_vec);
                    let create_key_button = if access_keys.len() < 2 {
                        Some(rsx! {
                            input {
                                "type": "button",
                                name: "CreateKey",
                                value: "CreateKey",
                                "onclick": "createAccessKey('{user_name}')"
                            }
                        })
                    } else {
                        None
                    };
                    let id = &u.user_id;
                    let cd = u.create_date;
                    let ar = &u.arn;
                    rsx! {
                        tr {
                            key: "user-key-{idx}",
                            style: "text-align: left;",
                            td {"{id}"},
                            td {"{cd}"},
                            td {"{user_name}"},
                            td {"{ar}"},
                            td {delete_button},
                            td {group_select},
                            td {group_remove_button},
                            td {create_key_button},
                        }
                    }
                })
            }
        }
    })
}

#[inline_props]
fn groups_element(
    cx: Scope,
    groups: Vec<IamGroup>,
    user_map: HashMap<StackString, HashSet<StackString>>,
    users: HashSet<StackString>,
) -> Element {
    let empty_set = HashSet::new();
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Group ID"},
                    th {"Create Date"},
                    th {"Group Name"},
                    th {"Arn"},
                }
            }
            groups.iter().enumerate().map(|(idx, g)| {
                let group_users = user_map.get(g.group_name.as_str()).unwrap_or(&empty_set);
                let group_name = &g.group_name;
                let user_opts: Vec<_> = users.iter().enumerate().filter_map(|(i, u)| {
                    if group_users.contains(u) {
                        None
                    } else {
                        Some(rsx! {
                            option {
                                key: "group-user-key-{i}",
                                value: "{u}",
                                "{u}"
                            },
                        })
                    }
                }).collect();
                let user_select = if user_opts.is_empty() {None} else {
                    Some(rsx! {
                        select {
                            id: "{group_name}_user_opt",
                            user_opts,
                        }
                    })
                };
                let user_add_button = if user_select.is_none() {None} else {
                    Some(rsx! {
                        input {
                            "type": "button",
                            name: "AddUser",
                            value: "Add",
                            "onclick": "addUserToGroup('{group_name}');",
                        }
                    })
                };
                let id = &g.group_id;
                let cd = g.create_date;
                let gn = &g.group_name;
                let ar = &g.arn;
                rsx! {
                    tr {
                        key: "group-key-{idx}",
                        style: "text-align: left;",
                        td {"{id}"},
                        td {"{cd}"},
                        td {"{gn}"},
                        td {"{ar}"},
                        td {user_select},
                        td {user_add_button},
                    }
                }
            })
        }
    })
}

#[inline_props]
fn access_key_element(cx: Scope, keys: Vec<AccessKeyMetadata>) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Key ID"},
                    th {"User Name"},
                    th {"Create Date"},
                    th {"Status"},
                }
            },
            tbody {
                keys.iter().enumerate().filter_map(|(idx, key)| {
                    let user_name = key.user_name.as_ref()?;
                    let access_key_id = key.access_key_id.as_ref()?;
                    let cd = key.create_date.as_ref()?;
                    let st = key.status.as_ref()?;
                    Some(rsx! {
                        tr {
                            key: "key-{idx}",
                            style: "text-align: left;",
                            td {"{access_key_id}"},
                            td {"{user_name}"},
                            td {"{cd}"},
                            td {"{st}"},
                            td {
                                input {
                                    "type": "button",
                                    name: "DeleteKey",
                                    value: "Delete",
                                    "onclick": "deleteAccessKey('{user_name}', '{access_key_id}');",
                                }
                            },
                        }
                    })
                })
            }
        }
    })
}

#[inline_props]
fn dns_record_element(
    cx: Scope,
    records: Vec<(String, String, String)>,
    current_ip: Ipv4Addr,
) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Zone ID"},
                    th {"DNS Name"},
                    th {"IP Address"},
                }
            },
            tbody {
                records.iter().enumerate().map(|(idx, (zone, name, ip))| {
                    rsx! {
                        tr {
                            key: "record-key-{idx}",
                            style: "text-align; left;",
                            td {"{zone}"},
                            td {"{name}"},
                            td {"{ip}"},
                            td {
                                input {
                                    "type": "button",
                                    name: "Update",
                                    value: "{current_ip}",
                                    "onclick": "updateDnsName('{zone}', '{name}.', '{ip}', '{current_ip}');",
                                }
                            },
                        }
                    }
                })
            }
        }
    })
}

#[inline_props]
fn systemd_element(
    cx: Scope,
    processes: HashMap<StackString, Vec<ProcessInfo>>,
    services: BTreeMap<StackString, RunStatus>,
    config: Config,
) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Name"},
                    th {"Status"},
                    th {
                        input {
                            "type": "button",
                            name: "RestartAll",
                            value: "RestartAll",
                            "onclick": "systemdRestartAll();",
                        }
                    }
                    th {
                        input {
                            "type": "button",
                            name: "Crontab",
                            value: "Crontab",
                            "onclick": "crontabLogs('user');"
                        },
                        br {},
                        input {
                            "type": "button",
                            name: "CrontabRoot",
                            value: "CrontabRoot",
                            "onclick": "crontabLogs('root');",
                        },
                    }
                    th {"Memory"},
                }
            },
            tbody {
                config.systemd_services.iter().enumerate().map(|(idx, service)| {
                    let proc_key = if service.len() > 15 {
                        &service[..15]
                    } else {
                        service
                    };
                    let run_status = services.get(service).unwrap_or(&RunStatus::NotRunning);
                    let proc_info = processes.get(proc_key);
                    let action_button = match run_status {
                        RunStatus::Running => {
                            rsx! {
                                input {
                                    "type": "button",
                                    name: "SystemdRestart",
                                    value: "Restart",
                                    "onclick": "systemdAction('restart', '{service}');",
                                },
                                input {
                                    "type": "button",
                                    name: "SystemdStop",
                                    value: "Stop",
                                    "onclick": "systemdAction('stop', '{service}');",
                                },
                            }
                        },
                        RunStatus::NotRunning => {
                            rsx! {
                                input {
                                    "type": "button",
                                    name: "SystemdStart",
                                    value: "Start",
                                    "onclick": "systemdAction('start', '{service}');",
                                }
                            }
                        }
                    };
                    let memory_info = proc_info.as_ref().map(|proc_info| {
                        let memory: u64 = proc_info.iter().map(|p| p.memory).sum();
                        let memory = memory as f32 / 1e6;
                        rsx! {"{memory:0.1} MiB"}
                    });
                    rsx! {
                        tr {
                            key: "systemd-key-{idx}",
                            style: "text-align; left;",
                            td {"{service}"},
                            td {"{run_status}"},
                            td {action_button},
                            td {
                                input {
                                    "type": "button",
                                    name: "SystemdLogs",
                                    value: "Logs",
                                    "onclick": "systemdLogs('{service}');",
                                }
                            },
                            td {memory_info},
                        }
                    }
                })
            }
        }
    })
}

pub fn instance_family_body(inst_fam: Vec<InstanceFamily>) -> String {
    let mut app = VirtualDom::new_with_props(
        instance_family_element,
        instance_family_elementProps { inst_fam },
    );
    app.rebuild();
    dioxus::ssr::render_vdom(&app)
}

#[inline_props]
fn instance_family_element(cx: Scope, inst_fam: Vec<InstanceFamily>) -> Element {
    cx.render(rsx! {
        br {
            form {
                action: "javascript:listPrices()",
                select {
                    id: "inst_fam",
                    "onchange": "listPrices();",
                    inst_fam.iter().enumerate().map(|(idx, fam)| {
                        let n = &fam.family_name;
                        let t = &fam.family_type;
                        rsx! {
                            option {
                                key: "inst-fam-key-{idx}",
                                value: "{n}.",
                                "{n} : {t}",
                            }
                        }
                    })
                }
            }
        }
    })
}

pub fn prices_body(prices: Vec<AwsInstancePrice>) -> String {
    let mut app = VirtualDom::new_with_props(price_element, price_elementProps { prices });
    app.rebuild();
    dioxus::ssr::render_vdom(&app)
}

#[inline_props]
fn price_element(cx: Scope, prices: Vec<AwsInstancePrice>) -> Element {
    cx.render(rsx! {
        table {
            "border": "1",
            class: "dataframe",
            thead {
                tr {
                    th {"Instance Type"},
                    th {"Ondemand Price"},
                    th {"Spot Price"},
                    th {"Reserved Price"},
                    th {"N CPU"},
                    th {"Memory GiB"},
                    th {"Instance Family"},
                }
            },
            tbody {
                prices.iter().enumerate().map(|(idx, price)| {
                    let instance_type = &price.instance_type;
                    let ncpu = price.ncpu;
                    let memory = price.memory;
                    let instance_family = &price.instance_family;
                    rsx! {
                        tr {
                            key: "price-key-{idx}",
                            style: "text-align: center;",
                            td {
                                price.data_url.as_ref().map_or_else(
                                    || {rsx! {"{instance_type}"}},
                                    |data_url| {
                                        rsx! {
                                            a {
                                                href: "{data_url}",
                                                target: "_blank",
                                                "{instance_type}",
                                            }
                                        }
                                    }
                                )
                            },
                            td {
                                price.ondemand_price.map(|p| rsx! {"${p:0.4}/hr"})
                            },
                            td {
                                price.spot_price.map(|p| rsx! {"${p:0.4}/hr"})
                            },
                            td {
                                price.reserved_price.map(|p| rsx! {"${p:0.4}/hr"})
                            },
                            td {"{ncpu}"},
                            td {"{memory}"},
                            td {"{instance_family}"},
                            td {
                                input {
                                    "type": "button",
                                    name: "Request",
                                    value: "Request",
                                    "onclick": "buildSpotRequest(null, '{instance_type}', null)",
                                }
                            }
                        }
                    }
                })
            }
        }
    })
}

pub fn edit_script_body(fname: StackString, text: StackString) -> String {
    let mut app = VirtualDom::new_with_props(
        edit_script_element,
        edit_script_elementProps { fname, text },
    );
    app.rebuild();
    dioxus::ssr::render_vdom(&app)
}

#[inline_props]
fn edit_script_element(cx: Scope, fname: StackString, text: StackString) -> Element {
    let rows = text.split('\n').count() + 5;
    cx.render(rsx! {
        br {
            textarea {
                name: "message",
                id: "script_editor_form",
                rows: "{rows}",
                cols: "100",
                form: "script_edit_form",
                "{text}",
            }
        }
        form {
            id: "script_edit_form",
            input {
                "type": "button",
                name: "update",
                value: "Update",
                "onclick": "submitFormData('{fname}')",
            },
            input {
                "type": "button",
                name: "cancel",
                value: "Cancel",
                "onclick": "listResource('script')",
            }
            input {
                "type": "button",
                name: "request",
                value: "Request",
                "onclick": "updateScriptAndBuildSpotRequest('{fname}')",
            }

        }
    })
}

pub fn build_spot_request_body(
    amis: Vec<AmiInfo>,
    inst_fams: Vec<InstanceFamily>,
    instances: Vec<InstanceList>,
    files: Vec<StackString>,
    keys: Vec<(StackString, StackString)>,
    config: Config,
) -> String {
    let mut app = VirtualDom::new_with_props(
        build_spot_request_element,
        build_spot_request_elementProps {
            amis,
            inst_fams,
            instances,
            files,
            keys,
            config,
        },
    );
    app.rebuild();
    dioxus::ssr::render_vdom(&app)
}

#[inline_props]
fn build_spot_request_element(
    cx: Scope,
    amis: Vec<AmiInfo>,
    inst_fams: Vec<InstanceFamily>,
    instances: Vec<InstanceList>,
    files: Vec<StackString>,
    keys: Vec<(StackString, StackString)>,
    config: Config,
) -> Element {
    let sec = config.spot_security_group.as_ref().unwrap_or_else(|| {
        config
            .default_security_group
            .as_ref()
            .expect("NO DEFAULT_SECURITY_GROUP")
    });
    let price = config.max_spot_price;
    cx.render(rsx! {
        form {
            action: "javascript:createScript()",
            br {
                "Ami: ",
                select {
                    id: "ami",
                    amis.iter().enumerate().map(|(idx, ami)| {
                        let id = &ami.id;
                        let name = &ami.name;
                        rsx! {
                            option {
                                key: "ami-key-{idx}",
                                value: "{id}",
                                "{name}"
                            }
                        }
                    })
                }
            },
            br {
                "Instance family: ",
                select {
                    id: "inst_fam",
                    "onchange": "instanceOptions()",
                    inst_fams.iter().enumerate().map(|(idx, fam)| {
                        let n = &fam.family_name;
                        rsx! {
                            option {
                                key: "inst-fam-key-{idx}",
                                value: "{n}",
                                "{n}",
                            }
                        }
                    })
                }
            },
            br {
                "Instance type: ",
                select {
                    id: "instance_type",
                    instances.iter().enumerate().map(|(idx, i)| {
                        let i = &i.instance_type;
                        rsx! {
                            option {
                                key: "instance-type-key-{idx}",
                                value: "{i}",
                                "{i}",
                            }
                        }
                    })
                }
            },
            br {
                "Security group: ",
                input {
                    "type": "text",
                    name: "security_group",
                    id: "security_group",
                    value: "{sec}",
                }
            },
            br {
                "Script: ",
                select {
                    id: "script",
                    files.iter().enumerate().map(|(idx, f)| {
                        rsx! {
                            option {
                                key: "script-key-{idx}",
                                value: "{f}",
                                "{f}",
                            }
                        }
                    })
                }
            },
            br {
                "Key :",
                select {
                    id: "key",
                    keys.iter().enumerate().map(|(idx, (k, _))| {
                        rsx! {
                            option {
                                key: "key-{idx}",
                                value: "{k}",
                                "{k}",
                            }
                        }
                    })
                }
            },
            br {
                "Price: ",
                input {
                    "type": "text",
                    name: "price",
                    id: "price",
                    value: "{price}",
                }
            },
            br {
                "Name: ",
                input {
                    "type": "text",
                    name: "name",
                    id: "name",
                }
            },
            input {
                "type": "button",
                name: "create_request",
                value: "Request",
                "onclick": "requestSpotInstance();",
            }
        }
    })
}

pub fn textarea_body(entries: Vec<StackString>, id: StackString) -> String {
    let mut app =
        VirtualDom::new_with_props(textarea_element, textarea_elementProps { entries, id });
    app.rebuild();
    dioxus::ssr::render_vdom(&app)
}

#[inline_props]
fn textarea_element(cx: Scope, entries: Vec<StackString>, id: StackString) -> Element {
    let rows = entries.len() + 5;
    let text = entries.join("\n");
    cx.render(rsx! {
        textarea {
            autofocus: "true",
            readonly: "readonly",
            name: "message",
            id: "{id}",
            rows: "{rows}",
            cols: "100",
            "{text}",
        }
    })
}

pub fn instance_status_body(entries: Vec<StackString>, instance: StackString) -> String {
    let mut app = VirtualDom::new_with_props(
        instance_status_element,
        instance_status_elementProps { entries, instance },
    );
    app.rebuild();
    dioxus::ssr::render_vdom(&app)
}

#[inline_props]
fn instance_status_element(cx: Scope, entries: Vec<StackString>, instance: StackString) -> Element {
    let rows = entries.len() + 5;
    let text = entries.join("\n");
    cx.render(rsx! {
        form {
            action: "javascript:runCommand('{instance}')",
            input {
                "type": "text",
                name: "command_text",
                id: "command_text",
            },
            input {
                "type": "button",
                name: "run_command",
                value: "Run",
                "onclick": "runCommand('{instance}');",
            }
        }
        textarea {
            autofocus: "true",
            readonly: "readonly",
            name: "message",
            id: "diary_editor_form",
            rows: "{rows}",
            cols: "100",
            "{text}",
        }
    })
}
