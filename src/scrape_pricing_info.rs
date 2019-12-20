use chrono::Utc;
use failure::{err_msg, Error};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use reqwest::Url;
use serde::Deserialize;
use std::collections::HashMap;

use crate::models::{InstancePricingInsert, PricingType};
use crate::pgpool::PgPool;

pub fn scrape_pricing_info(ptype: PricingType, pool: &PgPool) -> Result<(), Error> {
    let url = extract_json_url(get_url(ptype)?)?;
    println!("url {}", url);
    let js: PricingJson = reqwest::get(url)?.json()?;
    let results = parse_json(js, ptype)?;
    println!("{}", results.len());
    results
        .into_par_iter()
        .map(|r| r.upsert_entry(pool).map(|_| ()))
        .collect()
}

fn get_url(ptype: PricingType) -> Result<Url, Error> {
    match ptype {
        PricingType::Reserved => {
            "https://aws.amazon.com/ec2/pricing/reserved-instances/pricing/".parse()
        }
        PricingType::OnDemand => "https://aws.amazon.com/ec2/pricing/on-demand/".parse(),
        PricingType::Spot => unimplemented!(),
    }
    .map_err(err_msg)
}

fn extract_json_url(url: Url) -> Result<Url, Error> {
    let body: String = reqwest::get(url)?.text()?;
    let condition = |l: &&str| l.contains("data-service-url") && l.contains("/linux/");
    body.split('\n')
        .filter(condition)
        .nth(0)
        .and_then(|line| {
            line.split_whitespace()
                .filter(condition)
                .nth(0)
                .and_then(|entry| {
                    entry.split('=').nth(1).and_then(|s| {
                        s.replace('"', "")
                            .replace(r#"{{region}}"#, "us-east-1")
                            .parse()
                            .ok()
                    })
                })
        })
        .ok_or_else(|| err_msg("No url"))
}

fn parse_json(
    js: PricingJson,
    ptype: PricingType,
) -> Result<Vec<InstancePricingInsert<'static>>, Error> {
    println!("prices {}", js.prices.len());
    let _empty = "".to_string();
    js.prices
        .into_iter()
        .filter(|p| match ptype {
            PricingType::OnDemand => true,
            PricingType::Spot => false,
            PricingType::Reserved => {
                p.attributes
                    .get("aws:offerTermLeaseLength")
                    .unwrap_or_else(|| &_empty)
                    == "1yr"
                    && p.attributes
                        .get("aws:offerTermPurchaseOption")
                        .unwrap_or_else(|| &_empty)
                        == "All Upfront"
                    && p.attributes
                        .get("aws:offerTermOfferingClass")
                        .unwrap_or_else(|| &_empty)
                        == "standard"
            }
        })
        .map(|p| get_instance_pricing(p, ptype))
        .collect()
}

fn get_instance_pricing(
    price_entry: PricingEntry,
    ptype: PricingType,
) -> Result<InstancePricingInsert<'static>, Error> {
    match ptype {
        PricingType::OnDemand => {
            let price: f64 = price_entry
                .price
                .get("USD")
                .ok_or_else(|| err_msg("No USD Price"))?
                .parse()?;
            let instance_type = price_entry
                .attributes
                .get("aws:ec2:instanceType")
                .ok_or_else(|| err_msg("No instance type"))?
                .to_string();
            let i = InstancePricingInsert {
                instance_type: instance_type.into(),
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
                .ok_or_else(|| err_msg("No price"))?;
            let instance_type: String = price_entry
                .attributes
                .get("aws:ec2:instanceType")
                .ok_or_else(|| err_msg("No instance type"))?
                .to_string();
            let i = InstancePricingInsert {
                instance_type: instance_type.into(),
                price,
                price_type: "reserved".into(),
                price_timestamp: Utc::now(),
            };
            Ok(i)
        }
        PricingType::Spot => Err(err_msg("nothing")),
    }
}

#[derive(Deserialize)]
struct PricingEntry {
    price: HashMap<String, String>,
    attributes: HashMap<String, String>,
    #[serde(rename = "calculatedPrice")]
    calculated_price: Option<HashMap<String, HashMap<String, f64>>>,
}

#[derive(Deserialize)]
struct PricingJson {
    prices: Vec<PricingEntry>,
}
