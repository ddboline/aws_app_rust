use failure::{err_msg, Error};
use reqwest::Url;
use select::document::Document;
use select::node::Node;
use select::predicate::{Class, Name};

use crate::models::{AwsGeneration, InstanceFamilyInsert, InstanceList};
use crate::pgpool::PgPool;

pub fn scrape_instance_info(generation: AwsGeneration, pool: &PgPool) -> Result<(), Error> {
    let url: Url = match generation {
        AwsGeneration::HVM => "https://aws.amazon.com/ec2/instance-types/",
        AwsGeneration::PV => "https://aws.amazon.com/ec2/previous-generation/",
    }
    .parse()?;

    let body = reqwest::get(url)?.text()?;
    parse_result(&body, generation, pool)?;
    Ok(())
}

fn parse_result(text: &str, generation: AwsGeneration, pool: &PgPool) -> Result<(), Error> {
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
                    if family_name.contains(" ") {
                        continue;
                    }
                    let ifam = InstanceFamilyInsert {
                        family_name: family_name.into(),
                        family_type: family_type.to_string().into(),
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

    for t in &instance_families {
        if t.insert_entry(&pool)? {
            println!("{:?}", t);
        }
    }
    for t in &instance_types {
        if t.insert_entry(&pool)? {
            println!("{:?}", t);
        }
    }
    Ok(())
}

fn extract_instance_types_pv<'a>(
    table: &Node,
) -> Result<(Vec<InstanceFamilyInsert<'a>>, Vec<InstanceList<'a>>), Error> {
    let allowed_columns = ["Instance Family", "Instance Type", "vCPU", "Memory (GiB)"];
    let rows: Vec<_> = table
        .find(Name("tr"))
        .filter_map(|tr| {
            let row: Vec<_> = tr
                .find(Name("td"))
                .map(|td| td.text().trim().to_string())
                .collect();
            if row.len() > 0 && !row.iter().all(|x| x == "") {
                Some(row)
            } else {
                let row: Vec<_> = tr
                    .find(Name("th"))
                    .map(|th| th.text().trim().to_string())
                    .collect();
                if row.iter().all(|x| x == "") {
                    return None;
                }

                if row.len() > 0 {
                    Some(row)
                } else {
                    None
                }
            }
        })
        .collect();
    if rows.len() > 1 {
        let mut final_indicies: [isize; 4] = [-1; 4];
        for (idx, name) in allowed_columns.iter().enumerate() {
            for (idy, col) in rows[0].iter().enumerate() {
                if col == name {
                    final_indicies[idx] = idy as isize;
                }
            }
        }

        if final_indicies.iter().all(|x| *x != -1) {
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
    indicies: [isize; 4],
) -> Result<InstanceFamilyInsert<'static>, Error> {
    let family_type = row[indicies[0] as usize].to_string();
    let family_name = row[indicies[1] as usize]
        .split(".")
        .nth(0)
        .ok_or_else(|| err_msg("No family type"))?
        .to_string();
    Ok(InstanceFamilyInsert {
        family_name: family_name.into(),
        family_type: family_type.into(),
    })
}

fn extract_instance_types_hvm<'a>(table: &Node) -> Result<Vec<InstanceList<'a>>, Error> {
    let allowed_columns = [
        ["Instance Type", "vCPU", "Mem (GiB)"],
        ["Instance Type", "vCPU", "Memory (GiB)"],
        ["Model", "vCPU", "Mem (GiB)"],
        ["Model", "vCPU*", "Mem (GiB)"],
        ["Model", "Logical Proc*", "Mem (TiB)"],
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
            if row.len() > 0 && !row.iter().all(|x| x == "") {
                Some(row)
            } else {
                let row: Vec<_> = tr
                    .find(Name("th"))
                    .map(|th| th.text().trim().to_string())
                    .collect();
                if row.iter().all(|x| x == "") {
                    return None;
                }

                if row.len() > 0 {
                    Some(row)
                } else {
                    None
                }
            }
        })
        .collect();
    if rows.len() > 1 {
        let mut final_indicies: [isize; 3] = [-1; 3];
        for cols in allowed_columns.iter() {
            let mut indicies: [isize; 3] = [-1; 3];
            for (idx, name) in cols.iter().enumerate() {
                for (idy, col) in rows[0].iter().enumerate() {
                    if col == name {
                        indicies[idx] = idy as isize;
                    }
                }
            }
            if indicies.iter().all(|x| *x != -1) {
                final_indicies = indicies;
                break;
            }
        }
        if final_indicies.iter().any(|x| *x == -1) {
            println!("{:?}", rows[0]);
            println!("{:?}", rows[1]);
            println!("{:?}", final_indicies);
        } else {
            return rows[1..]
                .iter()
                .map(
                    |row| match extract_instance_type_object_hvm(row, final_indicies) {
                        Ok(x) => {
                            if x.instance_type == "1" || x.instance_type == "8" {
                                println!("{:?}", final_indicies);
                                println!("{:?}", rows[0]);
                                println!("row {:?}", row);
                            }
                            Ok(x)
                        }
                        Err(e) => {
                            println!("{:?}", final_indicies);
                            println!("{:?}", row);
                            Err(e)
                        }
                    },
                )
                .collect();
        }
    }
    Ok(Vec::new())
}

fn extract_instance_type_object_hvm(
    row: &[String],
    indicies: [isize; 3],
) -> Result<InstanceList<'static>, Error> {
    let mut instance_type: String = row[indicies[0] as usize].replace("*", "").to_string();
    let mut n_cpu: i32 = row[indicies[1] as usize].replace("*", "").parse()?;
    let mut memory_gib: f64 = row[indicies[2] as usize].replace(",", "").parse()?;

    if let Ok(_) = row[indicies[0] as usize].parse::<i32>() {
        instance_type = row[(indicies[0] - 1) as usize].replace("*", "").to_string();
        n_cpu = row[(indicies[1] - 1) as usize].replace("*", "").parse()?;
        memory_gib = row[(indicies[2] - 1) as usize].replace(",", "").parse()?;
    }

    Ok(InstanceList {
        instance_type: instance_type.into(),
        n_cpu,
        memory_gib,
        generation: AwsGeneration::HVM.to_string().into(),
    })
}

fn extract_instance_type_object_pv(
    row: &[String],
    indicies: [isize; 4],
) -> Result<InstanceList<'static>, Error> {
    let mut instance_type: String = row[indicies[1] as usize].replace("*", "").to_string();
    let mut n_cpu: i32 = row[indicies[2] as usize].replace("*", "").parse()?;
    let mut memory_gib: f64 = row[indicies[3] as usize].replace(",", "").parse()?;

    if let Ok(_) = row[indicies[1] as usize].parse::<i32>() {
        instance_type = row[(indicies[1] - 1) as usize].replace("*", "").to_string();
        n_cpu = row[(indicies[2] - 1) as usize].replace("*", "").parse()?;
        memory_gib = row[(indicies[3] - 1) as usize].replace(",", "").parse()?;
    }

    Ok(InstanceList {
        instance_type: instance_type.into(),
        n_cpu,
        memory_gib,
        generation: AwsGeneration::PV.to_string().into(),
    })
}
