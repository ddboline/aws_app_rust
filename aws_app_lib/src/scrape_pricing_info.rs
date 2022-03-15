use anyhow::{format_err, Error};
use chrono::Utc;
use futures::future::try_join_all;
use log::debug;
use reqwest::Url;
use serde::Deserialize;
use stack_string::{format_sstr, StackString};
use std::{collections::HashMap, fmt::Write};

use crate::{
    models::{InstancePricing, PricingType},
    pgpool::PgPool,
};

/// # Errors
/// Returns error if api call fails
pub async fn scrape_pricing_info(
    ptype: PricingType,
    pool: &PgPool,
) -> Result<Vec<StackString>, Error> {
    let mut output = Vec::new();
    let url = extract_json_url(get_url(ptype)?).await?;
    output.push(format_sstr!("url {url}"));
    let js: PricingJson = reqwest::get(url).await?.json().await?;
    let results = parse_json(js, ptype);
    output.push(format_sstr!("{}", results.len()));

    let results = results
        .into_iter()
        .map(|r| async move { r.upsert_entry(pool).await });
    try_join_all(results).await?;
    Ok(output)
}

/// # Errors
/// Returns error if `Url::parse` fails
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

fn parse_json_url_body(body: impl AsRef<str>) -> Result<Url, Error> {
    let condition = |l: &&str| l.contains("data-service-url");
    body.as_ref()
        .split('\n')
        .find(condition)
        .and_then(|line| {
            line.split_whitespace().find(condition).and_then(|entry| {
                entry.split('=').nth(1).and_then(|s| {
                    s.replace(r#"{{region}}"#, "us-east-1")
                        .trim_matches('"')
                        .parse()
                        .ok()
                })
            })
        })
        .ok_or_else(|| format_err!("No url"))
}

fn parse_json(js: PricingJson, ptype: PricingType) -> Vec<InstancePricing> {
    fn preserved_filter(p: &PricingEntry) -> bool {
        fn _cmp(os: Option<&StackString>, s: &str) -> bool {
            os.map(Into::into) == Some(s)
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
                get_instance_pricing(&p, ptype).ok()
            } else {
                None
            }
        })
        .collect()
}

fn get_instance_pricing(
    price_entry: &PricingEntry,
    ptype: PricingType,
) -> Result<InstancePricing, Error> {
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
                .ok_or_else(|| format_err!("No instance type {:?}", price_entry))?;
            let i = InstancePricing::new(instance_type.as_str(), price, "ondemand", Utc::now());
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
                .ok_or_else(|| format_err!("No instance type"))?;
            let i = InstancePricing::new(instance_type.as_str(), price, "reserved", Utc::now());
            Ok(i)
        }
        PricingType::Spot => Err(format_err!("nothing")),
    }
}

#[derive(Deserialize, Debug)]
struct PricingEntry {
    price: HashMap<StackString, StackString>,
    attributes: HashMap<StackString, StackString>,
    #[serde(rename = "calculatedPrice")]
    calculated_price: Option<HashMap<StackString, HashMap<StackString, f64>>>,
}

#[derive(Deserialize, Debug)]
struct PricingJson {
    prices: Vec<PricingEntry>,
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use flate2::read::GzDecoder;

    use crate::{
        models::PricingType,
        scrape_pricing_info::{parse_json, parse_json_url_body, PricingJson},
    };

    #[test]
    fn test_parse_json_url_body() -> Result<(), Error> {
        let reserved = include_str!("../../tests/data/reserved_pricing.html");
        let result = parse_json_url_body(reserved)?;
        assert_eq!(result.as_str(), "https://a0.p.awsstatic.com/pricing/1.0/ec2/region/us-east-1/reserved-instance/linux/index.json");

        let on_demand = include_str!("../../tests/data/on_demand.html");
        let result = parse_json_url_body(on_demand)?;
        assert_eq!(
            result.as_str(),
            "https://a0.p.awsstatic.com/pricing/1.0/ec2/region/us-east-1/ondemand/linux/index.json"
        );
        Ok(())
    }

    #[test]
    fn test_parse_json() -> Result<(), Error> {
        let data = include_bytes!("../../tests/data/reserved_instance.json.gz");
        let gz = GzDecoder::new(&data[..]);
        let js: PricingJson = serde_json::from_reader(gz)?;
        let ptype = PricingType::Reserved;
        let results = parse_json(js, ptype);
        assert_eq!(results.len(), 263);

        let data = include_bytes!("../../tests/data/ondemand.json.gz");
        let gz = GzDecoder::new(&data[..]);
        let js: PricingJson = serde_json::from_reader(gz)?;
        let ptype = PricingType::OnDemand;
        let results = parse_json(js, ptype);
        assert_eq!(results.len(), 263);
        Ok(())
    }
}
