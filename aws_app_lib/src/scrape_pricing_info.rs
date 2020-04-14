use anyhow::{format_err, Error};
use chrono::Utc;
use futures::future::try_join_all;
use log::debug;
use reqwest::Url;
use serde::Deserialize;
use std::collections::HashMap;

use crate::{
    models::{InstancePricingInsert, PricingType},
    pgpool::PgPool,
    stack_string::StackString,
};

pub async fn scrape_pricing_info(
    ptype: PricingType,
    pool: &PgPool,
) -> Result<Vec<StackString>, Error> {
    let mut output = Vec::new();
    let url = extract_json_url(get_url(ptype)?).await?;
    output.push(format!("url {}", url).into());
    let js: PricingJson = reqwest::get(url).await?.json().await?;
    let results = parse_json(js, ptype)?;
    output.push(format!("{}", results.len()).into());

    let results = results.into_iter().map(|r| r.upsert_entry(pool));
    try_join_all(results).await?;
    Ok(output)
}

fn get_url(ptype: PricingType) -> Result<Url, Error> {
    match ptype {
        PricingType::Reserved => {
            "https://aws.amazon.com/ec2/pricing/reserved-instances/pricing/".parse()
        }
        PricingType::OnDemand => "https://aws.amazon.com/ec2/pricing/on-demand/".parse(),
        PricingType::Spot => unimplemented!(),
    }
    .map_err(Into::into)
}

async fn extract_json_url(url: Url) -> Result<Url, Error> {
    let body = reqwest::get(url).await?.text().await?;
    parse_json_url_body(&body)
}

fn parse_json_url_body(body: &str) -> Result<Url, Error> {
    let condition = |l: &&str| l.contains("data-service-url") && l.contains("/linux/");
    body.split('\n')
        .find(condition)
        .and_then(|line| {
            line.split_whitespace().find(condition).and_then(|entry| {
                entry.split('=').nth(1).and_then(|s| {
                    s.replace('"', "")
                        .replace(r#"{{region}}"#, "us-east-1")
                        .parse()
                        .ok()
                })
            })
        })
        .ok_or_else(|| format_err!("No url"))
}

fn parse_json(js: PricingJson, ptype: PricingType) -> Result<Vec<InstancePricingInsert>, Error> {
    fn preserved_filter(p: &PricingEntry) -> bool {
        fn _cmp(os: Option<&StackString>, s: &str) -> bool {
            os.map(StackString::as_str) == Some(s)
        }
        _cmp(p.attributes.get("aws:offerTermLeaseLength"), "1yr")
            && _cmp(
                p.attributes.get("aws:offerTermPurchaseOption"),
                "All Upfront",
            )
            && _cmp(p.attributes.get("aws:offerTermOfferingClass"), "standard")
    }

    debug!("prices {}", js.prices.len());
    js.prices
        .into_iter()
        .filter_map(|p| {
            let get_price = match ptype {
                PricingType::OnDemand => true,
                PricingType::Spot => false,
                PricingType::Reserved => preserved_filter(&p),
            };
            if get_price {
                Some(get_instance_pricing(&p, ptype))
            } else {
                None
            }
        })
        .collect()
}

fn get_instance_pricing(
    price_entry: &PricingEntry,
    ptype: PricingType,
) -> Result<InstancePricingInsert, Error> {
    match ptype {
        PricingType::OnDemand => {
            let price: f64 = price_entry
                .price
                .get("USD")
                .ok_or_else(|| format_err!("No USD Price"))?
                .parse()?;
            let instance_type = price_entry
                .attributes
                .get("aws:ec2:instanceType")
                .ok_or_else(|| format_err!("No instance type"))?
                .clone();
            let i = InstancePricingInsert {
                instance_type,
                price,
                price_type: "ondemand".into(),
                price_timestamp: Utc::now(),
            };
            Ok(i)
        }
        PricingType::Reserved => {
            let price = *price_entry
                .calculated_price
                .as_ref()
                .and_then(|x| x.get("effectiveHourlyRate"))
                .and_then(|x| x.get("USD"))
                .ok_or_else(|| format_err!("No price"))?;
            let instance_type = price_entry
                .attributes
                .get("aws:ec2:instanceType")
                .ok_or_else(|| format_err!("No instance type"))?
                .to_owned();
            let i = InstancePricingInsert {
                instance_type,
                price,
                price_type: "reserved".into(),
                price_timestamp: Utc::now(),
            };
            Ok(i)
        }
        PricingType::Spot => Err(format_err!("nothing")),
    }
}

#[derive(Deserialize)]
struct PricingEntry {
    price: HashMap<StackString, StackString>,
    attributes: HashMap<StackString, StackString>,
    #[serde(rename = "calculatedPrice")]
    calculated_price: Option<HashMap<StackString, HashMap<StackString, f64>>>,
}

#[derive(Deserialize)]
struct PricingJson {
    prices: Vec<PricingEntry>,
}
