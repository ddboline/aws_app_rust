use anyhow::{format_err, Error};
use futures::future::try_join_all;
use log::debug;
use reqwest::Url;
use select::{
    document::Document,
    node::Node,
    predicate::{Class, Name},
};
use stack_string::StackString;
use std::collections::HashMap;

use crate::{
    models::{AwsGeneration, InstanceFamilyInsert, InstanceList},
    pgpool::PgPool,
};

pub fn get_url(generation: AwsGeneration) -> Result<Url, Error> {
    match generation {
        AwsGeneration::HVM => "https://aws.amazon.com/ec2/instance-types/",
        AwsGeneration::PV => "https://aws.amazon.com/ec2/previous-generation/",
    }
    .parse()
    .map_err(Into::into)
}

pub async fn scrape_instance_info(
    generation: AwsGeneration,
    pool: &PgPool,
) -> Result<Vec<StackString>, Error> {
    let url = get_url(generation)?;
    let body = reqwest::get(url).await?.text().await?;
    let (families, types) = parse_result(&body, generation)?;
    insert_result(families, types, pool).await
}

fn parse_result(
    text: &str,
    generation: AwsGeneration,
) -> Result<(Vec<InstanceFamilyInsert>, Vec<InstanceList>), Error> {
    let mut instance_families = Vec::new();
    let mut instance_types = Vec::new();
    let mut data_urls = HashMap::new();
    let doc = Document::from(text);

    match generation {
        AwsGeneration::HVM => {
            for c in doc.find(Class("lb-grid")) {
                let family_type: StackString = if let Some(d) = c.find(Class("lb-title")).last() {
                    d.text().trim().into()
                } else {
                    continue;
                };

                for d in c.find(Class("lb-txt-none")) {
                    let family_name: StackString = d.text().trim().to_lowercase().into();
                    if family_name.contains(' ') {
                        continue;
                    }
                    let ifam = InstanceFamilyInsert {
                        family_name,
                        family_type: family_type.clone(),
                        data_url: None,
                    };
                    instance_families.push(ifam);
                }

                for a in c.find(Name("a")) {
                    if let Some(url) = a.attr("href") {
                        if url.contains("instance-types") {
                            let url = url.replace("https://aws.amazon.com", "");
                            if let Some(key) = url.split("/").nth(3) {
                                data_urls.insert(key.to_string(), url);
                            }
                        }
                    }
                }
            }
            for t in doc.find(Name("tbody")) {
                instance_types.extend_from_slice(&extract_instance_types_hvm(&t)?);
            }
            for ifam in instance_families.iter_mut() {
                if let Some(url) = data_urls.get(ifam.family_name.as_str()) {
                    ifam.data_url.replace(url.into());
                } else {
                    for (key, url) in &data_urls {
                        if ifam.family_name.contains(key) {
                            ifam.data_url.replace(url.into());
                            break;
                        }
                    }
                }
            }
        }
        AwsGeneration::PV => {
            for t in doc.find(Name("tbody")) {
                let (inst_fam, inst_list) = extract_instance_types_pv(&t)?;
                instance_families.extend_from_slice(&inst_fam);
                instance_types.extend_from_slice(&inst_list);
            }
        }
    }
    instance_families.sort_by(|x, y| x.family_name.cmp(&y.family_name));
    instance_families.dedup_by(|x, y| x.family_name == y.family_name);

    Ok((instance_families, instance_types))
}

async fn insert_result(
    instance_families: Vec<InstanceFamilyInsert>,
    instance_types: Vec<InstanceList>,
    pool: &PgPool,
) -> Result<Vec<StackString>, Error> {
    let fam = instance_families.into_iter().map(|t| async {
        if let (t, true) = t.insert_entry(&pool).await? {
            Ok(Some(format!("{:?}", t).into()))
        } else {
            Ok(None)
        }
    });
    let fam: Result<Vec<_>, Error> = try_join_all(fam).await;
    let typ = instance_types.into_iter().map(|t| async {
        if let (t, true) = t.insert_entry(&pool).await? {
            Ok(Some(format!("{:?}", t).into()))
        } else {
            Ok(None)
        }
    });
    let typ: Result<Vec<_>, Error> = try_join_all(typ).await;
    let output: Vec<_> = fam?
        .into_iter()
        .chain(typ?.into_iter())
        .filter_map(|x| x)
        .collect();
    Ok(output)
}

#[derive(Debug, Clone, Copy)]
struct ColumnIndicies {
    instance_family: usize,
    instance_type: usize,
    n_cpu: usize,
    memory: usize,
}

fn extract_instance_types_pv(
    table: &Node,
) -> Result<(Vec<InstanceFamilyInsert>, Vec<InstanceList>), Error> {
    fn indicies_to_struct(indicies: &[Option<usize>; 4]) -> Option<ColumnIndicies> {
        Some(ColumnIndicies {
            instance_family: indicies[0]?,
            instance_type: indicies[1]?,
            n_cpu: indicies[2]?,
            memory: indicies[3]?,
        })
    }

    let allowed_columns = ["Instance Family", "Instance Type", "vCPU", "Memory (GiB)"];
    let rows: Vec<_> = table
        .find(Name("tr"))
        .filter_map(|tr| {
            let row: Vec<_> = tr
                .find(Name("td"))
                .map(|td| td.text().trim().to_string())
                .collect();
            if !row.is_empty() && !row.iter().all(|x| x == "") {
                Some(row)
            } else {
                let row: Vec<_> = tr
                    .find(Name("th"))
                    .map(|th| th.text().trim().to_string())
                    .collect();
                if row.iter().all(|x| x == "") {
                    return None;
                }

                if row.is_empty() {
                    None
                } else {
                    Some(row)
                }
            }
        })
        .collect();
    if rows.len() > 1 {
        let mut final_indicies: [Option<usize>; 4] = [None; 4];
        for (idx, name) in allowed_columns.iter().enumerate() {
            for (idy, col) in rows[0].iter().enumerate() {
                if col == name {
                    final_indicies[idx] = Some(idy);
                }
            }
        }
        if let Some(final_indicies) = indicies_to_struct(&final_indicies) {
            let instance_families = rows[1..]
                .iter()
                .map(|row| extract_instance_family_object_pv(row, final_indicies))
                .collect::<Result<Vec<_>, Error>>()?;
            let instance_types = rows[1..]
                .iter()
                .map(|row| extract_instance_type_object_pv(row, final_indicies))
                .collect::<Result<Vec<_>, Error>>()?;
            return Ok((instance_families, instance_types));
        }
    }
    Ok((Vec::new(), Vec::new()))
}

fn extract_instance_family_object_pv(
    row: &[impl AsRef<str>],
    indicies: ColumnIndicies,
) -> Result<InstanceFamilyInsert, Error> {
    let family_type = row[indicies.instance_family].as_ref().into();
    let family_name = row[indicies.instance_type]
        .as_ref()
        .split('.')
        .next()
        .ok_or_else(|| format_err!("No family type"))?
        .into();
    Ok(InstanceFamilyInsert {
        family_name,
        family_type,
        data_url: None,
    })
}

fn extract_instance_types_hvm(table: &Node) -> Result<Vec<InstanceList>, Error> {
    fn indicies_to_struct(indicies: &[Option<usize>; 3]) -> Option<ColumnIndicies> {
        Some(ColumnIndicies {
            instance_family: 0,
            instance_type: indicies[0]?,
            n_cpu: indicies[1]?,
            memory: indicies[2]?,
        })
    }

    let allowed_columns = [
        ["Instance Type", "vCPU", "Mem (GiB)"],
        ["Instance Type", "vCPU", "Memory (GiB)"],
        ["Model", "vCPU", "Mem (GiB)"],
        ["Model", "vCPU*", "Mem (GiB)"],
        ["Model", "Logical Proc*", "Mem (TiB)"],
        ["Model", "vCPU", "Memory (GiB)"],
        ["Instance", "vCPU", "Mem (GiB)"],
        ["Instance", "vCPU*", "Mem (GiB)"],
        ["Instance", "vCPU", "Memory (GiB)"],
        ["Instance", "Logical Proc*", "Mem (TiB)"],
        ["Name", "Logical Processors*", "RAM (GiB)"],
        ["Instance", "vCPU", "Mem (GB)"],
        ["Instance Size", "vCPU", "Memory (GiB)"],
    ];

    let rows: Vec<_> = table
        .find(Name("tr"))
        .filter_map(|tr| {
            let row: Vec<_> = tr
                .find(Name("td"))
                .map(|td| td.text().trim().to_string())
                .collect();
            if !row.is_empty() && !row.iter().all(|x| x == "") {
                Some(row)
            } else {
                let row: Vec<_> = tr
                    .find(Name("th"))
                    .map(|th| th.text().trim().to_string())
                    .collect();
                if row.iter().all(|x| x == "") {
                    return None;
                }

                if row.is_empty() {
                    None
                } else {
                    Some(row)
                }
            }
        })
        .collect();
    if rows.len() < 2 {
        return Ok(Vec::new());
    }
    let mut final_indicies = None;
    for cols in &allowed_columns {
        let mut indicies: [Option<usize>; 3] = [None; 3];
        for (idx, name) in cols.iter().enumerate() {
            for (idy, col) in rows[0].iter().enumerate() {
                if col == name {
                    indicies[idx] = Some(idy);
                }
            }
        }
        if let Some(indicies) = indicies_to_struct(&indicies) {
            final_indicies = Some(indicies);
            break;
        }
    }
    if let Some(final_indicies) = final_indicies {
        rows[1..]
            .iter()
            .map(
                |row| match extract_instance_type_object_hvm(row, final_indicies) {
                    Ok(x) => {
                        if &x.instance_type == "1" || &x.instance_type == "8" {
                            debug!("{:?}", final_indicies);
                            debug!("{:?}", rows[0]);
                            debug!("row {:?}", row);
                        }
                        Ok(x)
                    }
                    Err(e) => {
                        debug!("{:?}", final_indicies);
                        debug!("{:?}", row);
                        Err(e)
                    }
                },
            )
            .collect()
    } else {
        debug!("{:?}", rows[0]);
        debug!("{:?}", rows[1]);
        debug!("{:?}", final_indicies);
        Ok(Vec::new())
    }
}

fn extract_instance_type_object_hvm(
    row: &[impl AsRef<str>],
    indicies: ColumnIndicies,
) -> Result<InstanceList, Error> {
    let idx = if row[indicies.instance_type]
        .as_ref()
        .replace("*", "")
        .parse::<i32>()
        .is_ok()
    {
        1
    } else {
        0
    };

    let instance_type = row[(indicies.instance_type - idx)]
        .as_ref()
        .replace("*", "")
        .into();
    let n_cpu: i32 = row[(indicies.n_cpu - idx)]
        .as_ref()
        .replace("*", "")
        .parse()?;
    let memory_gib: f64 = row[(indicies.memory - idx)]
        .as_ref()
        .replace(",", "")
        .parse()?;

    Ok(InstanceList {
        instance_type,
        n_cpu,
        memory_gib,
        generation: AwsGeneration::HVM.into(),
    })
}

fn extract_instance_type_object_pv(
    row: &[impl AsRef<str>],
    indicies: ColumnIndicies,
) -> Result<InstanceList, Error> {
    let idx = if row[indicies.instance_type].as_ref().parse::<i32>().is_ok() {
        1
    } else {
        0
    };

    let instance_type = row[(indicies.instance_type - idx)]
        .as_ref()
        .replace("*", "")
        .into();
    let n_cpu: i32 = row[(indicies.n_cpu - idx)]
        .as_ref()
        .replace("*", "")
        .parse()?;
    let memory_gib: f64 = row[(indicies.memory - idx)]
        .as_ref()
        .replace(",", "")
        .parse()?;

    Ok(InstanceList {
        instance_type,
        n_cpu,
        memory_gib,
        generation: AwsGeneration::PV.to_string().into(),
    })
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use crate::{models::AwsGeneration, scrape_instance_info::parse_result};

    #[test]
    fn test_parse_result() -> Result<(), Error> {
        let text_hvm = include_str!("../../tests/data/instance_types_hvm.html");
        let (families, types) = parse_result(&text_hvm, AwsGeneration::HVM)?;
        assert_eq!(families.len(), 32);
        assert_eq!(types.len(), 264);
        let text_pv = include_str!("../../tests/data/instance_types_pv.html");
        let (families, types) = parse_result(&text_pv, AwsGeneration::PV)?;
        assert_eq!(families.len(), 12);
        assert_eq!(types.len(), 33);
        Ok(())
    }
}
