use anyhow::Error;
use rusoto_core::Region;
use rusoto_pricing::{
    DescribeServicesRequest, Filter, GetAttributeValuesRequest, GetProductsRequest, Pricing,
    PricingClient,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{collections::HashMap, fmt};
use stdout_channel::rate_limiter::RateLimiter;
use sts_profile_auth::get_client_sts;
use time::OffsetDateTime;

use crate::{
    config::Config,
    date_time_wrapper::DateTimeWrapper,
    models::{InstanceList, InstancePricing, PricingType},
    pgpool::PgPool,
};

#[derive(Clone)]
pub struct PricingInstance {
    pricing_client: PricingClient,
    limit: RateLimiter,
}

impl fmt::Debug for PricingInstance {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("PricingInstance")
    }
}

impl Default for PricingInstance {
    fn default() -> Self {
        Self {
            pricing_client: get_client_sts!(PricingClient, Region::UsEast1)
                .expect("StsProfile failed"),
            limit: RateLimiter::new(10, 5000),
        }
    }
}

impl PricingInstance {
    #[must_use]
    pub fn new(config: &Config) -> Self {
        let region: Region = config
            .aws_region_name
            .parse()
            .ok()
            .unwrap_or(Region::UsEast1);
        Self {
            pricing_client: get_client_sts!(PricingClient, region).expect("StsProfile failed"),
            limit: RateLimiter::new(10, 5000),
        }
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn describe_services(
        &self,
        service_code: Option<&str>,
    ) -> Result<HashMap<StackString, AwsService>, Error> {
        let mut next_token = None;
        let mut all_services = HashMap::new();
        loop {
            let input = DescribeServicesRequest {
                service_code: service_code.map(Into::into),
                next_token: next_token.clone(),
                ..DescribeServicesRequest::default()
            };
            self.limit.acquire().await;
            let mut result = self.pricing_client.describe_services(input).await?;
            if let Some(services) = result.services.take() {
                for service in services {
                    if let Some(service_code) = service.service_code.map(Into::<StackString>::into)
                    {
                        if let Some(attributes) = service.attribute_names {
                            let attributes = attributes.into_iter().map(Into::into).collect();
                            all_services.insert(
                                service_code.clone(),
                                AwsService {
                                    service_code,
                                    attributes,
                                },
                            );
                        }
                    }
                }
            }
            if let Some(token) = result.next_token.take() {
                next_token.replace(token);
            } else {
                break;
            }
        }
        Ok(all_services)
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn get_attribute_values(
        &self,
        service_code: &str,
        attribute_name: &str,
    ) -> Result<Vec<StackString>, Error> {
        let mut next_token = None;
        let mut results = Vec::new();
        loop {
            let input = GetAttributeValuesRequest {
                service_code: service_code.into(),
                attribute_name: attribute_name.into(),
                next_token: next_token.clone(),
                ..GetAttributeValuesRequest::default()
            };
            self.limit.acquire().await;
            let mut response = self.pricing_client.get_attribute_values(input).await?;
            if let Some(values) = response.attribute_values.take() {
                results.extend(
                    values
                        .into_iter()
                        .filter_map(|val| val.value.map(Into::into)),
                );
            }
            if let Some(token) = response.next_token.take() {
                next_token.replace(token);
            } else {
                break;
            }
        }
        Ok(results)
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn get_prices(
        &self,
        instance_type: &str,
    ) -> Result<HashMap<(StackString, PricingType), InstancePricing>, Error> {
        let mut next_token = None;
        let mut entries: HashMap<(StackString, PricingType), InstancePricing> = HashMap::new();
        loop {
            let input = GetProductsRequest {
                format_version: Some("aws_v1".into()),
                service_code: Some("AmazonEC2".into()),
                filters: Some(vec![
                    Filter {
                        field: "operatingSystem".into(),
                        type_: "TERM_MATCH".into(),
                        value: "Linux".into(),
                    },
                    Filter {
                        field: "instanceType".into(),
                        type_: "TERM_MATCH".into(),
                        value: instance_type.into(),
                    },
                    Filter {
                        field: "location".into(),
                        type_: "TERM_MATCH".into(),
                        value: "US East (N. Virginia)".into(),
                    },
                    Filter {
                        field: "OfferingClass".into(),
                        type_: "TERM_MATCH".into(),
                        value: "standard".into(),
                    },
                    Filter {
                        field: "locationType".into(),
                        type_: "TERM_MATCH".into(),
                        value: "AWS Region".into(),
                    },
                ]),
                next_token: next_token.clone(),
                ..GetProductsRequest::default()
            };
            self.limit.acquire().await;
            let mut response = self.pricing_client.get_products(input).await?;
            if let Some(price_list) = response.price_list.take() {
                for price in price_list {
                    #[derive(Deserialize, Debug)]
                    struct PricePerUnit<'a> {
                        #[serde(rename = "USD")]
                        usd: &'a str,
                    }
                    #[derive(Deserialize, Debug)]
                    struct PriceDimension<'a> {
                        unit: &'a str,
                        #[serde(rename = "pricePerUnit")]
                        price_per_unit: PricePerUnit<'a>,
                    }
                    #[derive(Deserialize, Debug)]
                    struct TermAttributes<'a> {
                        #[serde(rename = "LeaseContractLength")]
                        lease_contract_length: Option<&'a str>,
                        #[serde(rename = "PurchaseOption")]
                        purchase_option: Option<&'a str>,
                    }
                    #[derive(Deserialize, Debug)]
                    struct PriceDimensions<'a> {
                        #[serde(rename = "priceDimensions", borrow)]
                        price_dimensions: HashMap<&'a str, PriceDimension<'a>>,
                        #[serde(rename = "effectiveDate")]
                        effective_date: DateTimeWrapper,
                        #[serde(rename = "termAttributes")]
                        term_attributes: TermAttributes<'a>,
                    }
                    #[derive(Deserialize, Debug)]
                    struct PriceList<'a> {
                        #[serde(borrow)]
                        terms: HashMap<&'a str, HashMap<&'a str, PriceDimensions<'a>>>,
                    }

                    let value: PriceList = serde_json::from_str(&price)?;
                    if let Some(ondemand) = value.terms.get("OnDemand") {
                        for price_dimensions in ondemand.values() {
                            for dimension in price_dimensions.price_dimensions.values() {
                                if dimension.unit != "Hrs" {
                                    continue;
                                }
                                if let Ok(price) = dimension.price_per_unit.usd.parse::<f64>() {
                                    let price_type = PricingType::OnDemand;
                                    let price_timestamp: OffsetDateTime =
                                        price_dimensions.effective_date.into();
                                    let instance_type: StackString = instance_type.into();
                                    if let Some(i) =
                                        entries.get(&(instance_type.clone(), price_type))
                                    {
                                        if i.price_timestamp > price_timestamp {
                                            continue;
                                        }
                                    }
                                    let i = InstancePricing::new(
                                        instance_type.as_str(),
                                        price,
                                        price_type.to_str(),
                                        price_dimensions.effective_date.into(),
                                    );
                                    entries.insert((instance_type, price_type), i);
                                }
                            }
                        }
                    }
                    if let Some(reserved) = value.terms.get("Reserved") {
                        for price_dimensions in reserved.values() {
                            if price_dimensions.term_attributes.lease_contract_length != Some("1yr")
                            {
                                continue;
                            }
                            if price_dimensions.term_attributes.purchase_option
                                != Some("All Upfront")
                            {
                                continue;
                            }
                            for dimension in price_dimensions.price_dimensions.values() {
                                if dimension.unit != "Quantity" {
                                    continue;
                                }
                                if let Ok(price) = dimension.price_per_unit.usd.parse::<f64>() {
                                    if price == 0.0 {
                                        continue;
                                    }
                                    let price = price / (365.0 * 24.0);

                                    let price_type = PricingType::Reserved;
                                    let price_timestamp = price_dimensions.effective_date.into();
                                    let instance_type: StackString = instance_type.into();
                                    if let Some(i) =
                                        entries.get(&(instance_type.clone(), price_type))
                                    {
                                        if i.price_timestamp > price_timestamp {
                                            continue;
                                        }
                                    }
                                    let i = InstancePricing::new(
                                        instance_type.as_str(),
                                        price,
                                        price_type.to_str(),
                                        price_timestamp,
                                    );
                                    entries.insert((instance_type, price_type), i);
                                }
                            }
                        }
                    }
                }
            }
            if let Some(token) = response.next_token.take() {
                next_token.replace(token);
            } else {
                break;
            }
        }
        Ok(entries)
    }

    /// # Errors
    /// Returns error if aws api fails
    pub async fn update_all_prices(&self, pool: &PgPool) -> Result<u32, Error> {
        let instances: Vec<_> = InstanceList::get_all_instances(pool)
            .await?
            .into_iter()
            .map(|i| i.instance_type)
            .collect();
        let mut number_of_updates = 0;
        for instance in instances {
            for (_, price) in self.get_prices(&instance).await? {
                price.upsert_entry(pool).await?;
                number_of_updates += 1;
            }
        }
        Ok(number_of_updates)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AwsService {
    pub service_code: StackString,
    pub attributes: Vec<StackString>,
}

#[cfg(test)]
mod tests {
    use anyhow::Error;

    use crate::{config::Config, pricing_instance::PricingInstance};

    #[tokio::test]
    async fn test_describe_services() -> Result<(), Error> {
        let config = Config::init_config()?;
        let pricing = PricingInstance::new(&config);
        let services = pricing.describe_services(None).await?;
        assert_eq!(services.len(), 194);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_attributes() -> Result<(), Error> {
        let config = Config::init_config()?;
        let pricing = PricingInstance::new(&config);
        let ec2_service = pricing.describe_services(Some("AmazonEC2")).await?;
        let ec2_service = &ec2_service["AmazonEC2"];
        assert_eq!(ec2_service.attributes.len(), 80);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_attribute_values() -> Result<(), Error> {
        let config = Config::init_config()?;
        let pricing = PricingInstance::new(&config);
        let values = pricing
            .get_attribute_values("AmazonEC2", "operatingSystem")
            .await?;
        println!("{:?}", values);
        assert_eq!(values.len(), 6);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_prices() -> Result<(), Error> {
        let config = Config::init_config()?;
        let pricing = PricingInstance::new(&config);
        let prices = pricing.get_prices("t3.micro").await?;
        assert_eq!(prices.len(), 2);
        Ok(())
    }
}
