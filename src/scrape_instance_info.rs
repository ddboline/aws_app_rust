use failure::Error;
use reqwest::Url;
use select::document::Document;

use crate::models::AwsGeneration;

pub fn scrape_instance_info(url: Url, generation: AwsGeneration) -> Result<String, Error> {
    let body = reqwest::get(url)?.text()?;
    parse_result(&body, generation)?;
    Ok(body)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TablePlacement {
    Place0,
    Place1,
    Place2,
}

fn parse_result(text: &str, generation: AwsGeneration) -> Result<(), Error> {
    let table_start = match generation {
        AwsGeneration::HVM => "lb-tbl",
        AwsGeneration::PV => "Previous_Generation_Instance_Details",
    };

    let mut place = TablePlacement::Place0;
    let mut instance_family = "all".to_string();
    let mut htmltablestring = Vec::new();
    for line in text.split("\n") {
        let line = line.trim();
        if line.contains("lb-title") {
            if let Some(node) = Document::from(line).nth(0) {
                instance_family = node.text().trim().to_string();
            }
        }
        if place == TablePlacement::Place0 {
            if line.contains(table_start) {
                place = TablePlacement::Place1;
            } else {
                continue;
            }
        }
        if place == TablePlacement::Place1 {
            if line.contains("<table") {
                place = TablePlacement::Place2;
                htmltablestring.clear();
            } else {
                continue;
            }
        }
        if place == TablePlacement::Place2 {
            htmltablestring.push(line);
            if line.contains("</table>") {
                place = TablePlacement::Place0;
                let table = htmltablestring.join("\n");
                let doc = Document::from(table.as_str());
                println!("{}", table);
                htmltablestring.clear();
            }
        }
    }
    Ok(())
}
