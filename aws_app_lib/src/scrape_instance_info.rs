use anyhow::{format_err, Error};
use futures::future::try_join_all;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use reqwest::Url;
use select::document::Document;
use select::node::Node;
use select::predicate::{Class, Name};
use std::io::{stdout, Write};

use crate::models::{AwsGeneration, InstanceFamilyInsert, InstanceList};
use crate::pgpool::PgPool;

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
) -> Result<Vec<String>, Error> {
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
    let doc = Document::from(text);

    match generation {
        AwsGeneration::HVM => {
            for c in doc.find(Class("lb-grid")) {
                let mut family_type = "".to_string();
                for d in c.find(Class("lb-title")) {
                    family_type = d.text().trim().to_string();
                }
                if family_type == "" {
                    continue;
                }
                for d in c.find(Class("lb-txt-none")) {
                    let family_name = d.text().trim().to_lowercase();
                    if family_name.contains(' ') {
                        continue;
                    }
                    let ifam = InstanceFamilyInsert {
                        family_name,
                        family_type: family_type.to_string(),
                    };
                    instance_families.push(ifam);
                }
            }
            for t in doc.find(Name("tbody")) {
                instance_types.extend_from_slice(&extract_instance_types_hvm(&t)?);
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

    Ok((instance_families, instance_types))
}

async fn insert_result(
    instance_families: Vec<InstanceFamilyInsert>,
    instance_types: Vec<InstanceList>,
    pool: &PgPool,
) -> Result<Vec<String>, Error> {
    let fam: Vec<_> = instance_families
        .into_iter()
        .map(|t| async {
            if let (t, true) = t.insert_entry(&pool).await? {
                Ok(Some(format!("{:?}", t)))
            } else {
                Ok(None)
            }
        })
        .collect();
    let fam: Result<Vec<_>, Error> = try_join_all(fam).await;
    let typ: Vec<_> = instance_types
        .into_iter()
        .map(|t| async {
            if let (t, true) = t.insert_entry(&pool).await? {
                Ok(Some(format!("{:?}", t)))
            } else {
                Ok(None)
            }
        })
        .collect();
    let typ: Result<Vec<_>, Error> = try_join_all(typ).await;
    let output: Vec<_> = fam?
        .into_iter()
        .chain(typ?.into_iter())
        .filter_map(|x| x)
        .collect();
    Ok(output)
}

fn extract_instance_types_pv(
    table: &Node,
) -> Result<(Vec<InstanceFamilyInsert>, Vec<InstanceList>), Error> {
    let allowed_columns = ["Instance Family", "Instance Type", "vCPU", "Memory (GiB)"];
    let rows: Vec<_> = table
        .find(Name("tr"))
        .filter_map(|tr| {
            let row: Vec<_> = tr
                .find(Name("td"))
                .map(|td| td.text().trim().to_string())
                .collect();
            if !row.is_empty() && !row.par_iter().all(|x| x == "") {
                Some(row)
            } else {
                let row: Vec<_> = tr
                    .find(Name("th"))
                    .map(|th| th.text().trim().to_string())
                    .collect();
                if row.par_iter().all(|x| x == "") {
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
        let mut final_bitmap: u8 = 0x0;
        let mut final_indicies: [usize; 4] = [0; 4];
        for (idx, name) in allowed_columns.iter().enumerate() {
            for (idy, col) in rows[0].iter().enumerate() {
                if col == name {
                    final_bitmap |= 1 << idx;
                    final_indicies[idx] = idy;
                }
            }
        }
        if final_bitmap == 0xf {
            let mut instance_families = Vec::new();
            let mut instance_types = Vec::new();
            for row in &rows[1..] {
                instance_families.push(extract_instance_family_object_pv(row, final_indicies)?);
                instance_types.push(extract_instance_type_object_pv(row, final_indicies)?);
            }
            return Ok((instance_families, instance_types));
        }
    }
    Ok((Vec::new(), Vec::new()))
}

fn extract_instance_family_object_pv(
    row: &[String],
    indicies: [usize; 4],
) -> Result<InstanceFamilyInsert, Error> {
    let family_type = row[indicies[0]].to_string();
    let family_name = row[indicies[1]]
        .split('.')
        .nth(0)
        .ok_or_else(|| format_err!("No family type"))?
        .to_string();
    Ok(InstanceFamilyInsert {
        family_name,
        family_type,
    })
}

fn extract_instance_types_hvm(table: &Node) -> Result<Vec<InstanceList>, Error> {
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
            if !row.is_empty() && !row.par_iter().all(|x| x == "") {
                Some(row)
            } else {
                let row: Vec<_> = tr
                    .find(Name("th"))
                    .map(|th| th.text().trim().to_string())
                    .collect();
                if row.par_iter().all(|x| x == "") {
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
    let mut final_bitmap: u8 = 0x0;
    let mut final_indicies: [usize; 3] = [0; 3];
    for cols in &allowed_columns {
        let mut bitmap: u8 = 0x0;
        let mut indicies: [usize; 3] = [0; 3];
        for (idx, name) in cols.iter().enumerate() {
            for (idy, col) in rows[0].iter().enumerate() {
                if col == name {
                    bitmap |= 1 << idx;
                    indicies[idx] = idy;
                }
            }
        }
        if bitmap == 0x7 {
            final_bitmap = bitmap;
            final_indicies = indicies;
            break;
        }
    }
    if final_bitmap == 0x7 {
        rows[1..]
            .par_iter()
            .map(
                |row| match extract_instance_type_object_hvm(row, final_indicies) {
                    Ok(x) => {
                        if x.instance_type == "1" || x.instance_type == "8" {
                            writeln!(stdout(), "{:?}", final_indicies)?;
                            writeln!(stdout(), "{:?}", rows[0])?;
                            writeln!(stdout(), "row {:?}", row)?;
                        }
                        Ok(x)
                    }
                    Err(e) => {
                        writeln!(stdout(), "{:?}", final_indicies)?;
                        writeln!(stdout(), "{:?}", row)?;
                        Err(e)
                    }
                },
            )
            .collect()
    } else {
        writeln!(stdout(), "{:?}", rows[0])?;
        writeln!(stdout(), "{:?}", rows[1])?;
        writeln!(stdout(), "{:?}", final_indicies)?;
        Ok(Vec::new())
    }
}

fn extract_instance_type_object_hvm(
    row: &[String],
    indicies: [usize; 3],
) -> Result<InstanceList, Error> {
    let idx = if row[indicies[0]].replace("*", "").parse::<i32>().is_ok() {
        1
    } else {
        0
    };

    let instance_type: String = row[(indicies[0] - idx)].replace("*", "");
    let n_cpu: i32 = row[(indicies[1] - idx)].replace("*", "").parse()?;
    let memory_gib: f64 = row[(indicies[2] - idx)].replace(",", "").parse()?;

    Ok(InstanceList {
        instance_type,
        n_cpu,
        memory_gib,
        generation: AwsGeneration::HVM.to_string(),
    })
}

fn extract_instance_type_object_pv(
    row: &[String],
    indicies: [usize; 4],
) -> Result<InstanceList, Error> {
    let idx = if row[indicies[1]].parse::<i32>().is_ok() {
        1
    } else {
        0
    };

    let instance_type: String = row[(indicies[1] - idx)].replace("*", "");
    let n_cpu: i32 = row[(indicies[2] - idx)].replace("*", "").parse()?;
    let memory_gib: f64 = row[(indicies[3] - idx)].replace(",", "").parse()?;

    Ok(InstanceList {
        instance_type,
        n_cpu,
        memory_gib,
        generation: AwsGeneration::PV.to_string(),
    })
}
