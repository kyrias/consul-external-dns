use async_trait::async_trait;
use reqwest::{header, Client, Error};
use serde::{Deserialize, Serialize};

use crate::{
    config::HetznerConfig,
    consul,
    dns_trait::{DnsProviderTrait, DnsRecord, DnsRecordCreate},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct RecordsWrapper {
    records: Vec<DnsRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecordResponse {
    record: DnsRecord,
}

pub struct HetznerDns {
    pub config: HetznerConfig,
}

#[async_trait]
impl DnsProviderTrait for HetznerDns {
    /// Update or create a DNS record based on the Consul service tags
    /// If the record already exists, it will be updated if the value or ttl is different
    /// If the record does not exist, it will be created
    async fn update_or_create_dns_record<'a>(
        &self,
        dns_record: &'a consul::DnsRecord,
    ) -> Result<DnsRecord, anyhow::Error> {
        let existing_records = match list_dns_records(&self.config).await {
            Ok(records) => records,
            Err(e) => {
                eprintln!("Failed to list DNS records: {}", e);
                return Err(e.into());
            }
        };

        let matched_record = existing_records
            .iter()
            .find(|record| record.name == dns_record.hostname && record.type_ == dns_record.type_);

        match matched_record {
            Some(record) => {
                if record.value != dns_record.value || record.ttl != dns_record.ttl {
                    // Update the existing record
                    let updated_record = DnsRecord {
                        id: record.id.clone(),
                        zone_id: record.zone_id.clone(),
                        type_: dns_record.type_.clone(),
                        name: dns_record.hostname.clone(),
                        value: dns_record.value.clone(),
                        ttl: dns_record.ttl,
                    };
                    let updated_record = update_dns_record(&self.config, &updated_record).await?;
                    Ok(updated_record)
                } else {
                    Ok(record.clone())
                }
            }
            None => {
                // Create a new DNS record
                let new_record = DnsRecordCreate {
                    zone_id: self.config.dns_zone_id.clone(),
                    type_: dns_record.type_.clone(),
                    name: dns_record.hostname.clone(),
                    value: dns_record.value.clone(),
                    ttl: dns_record.ttl,
                };
                let created_record = create_dns_record(&self.config, &new_record).await?;
                Ok(created_record)
            }
        }
    }

    async fn delete_dns_record<'a>(&self, record_id: &'a str) -> Result<(), anyhow::Error> {
        let url = format!("{}/records/{}", &self.config.api_url, record_id);
        let client = Client::new();
        client
            .delete(url)
            .header("Auth-API-Token", &self.config.dns_token)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

async fn list_dns_records(hetzner_config: &HetznerConfig) -> Result<Vec<DnsRecord>, Error> {
    let client = Client::new();
    let mut headers = header::HeaderMap::new();
    headers.insert(
        "Auth-API-Token",
        header::HeaderValue::from_str(&hetzner_config.dns_token).unwrap(),
    );

    let url = format!(
        "{}/records?zone_id={}",
        &hetzner_config.api_url, &hetzner_config.dns_zone_id
    );
    let response = client.get(url).headers(headers).send().await?;

    match response.error_for_status() {
        Ok(res) => {
            let record = res.json::<RecordsWrapper>().await?;
            Ok(record.records)
        }
        Err(err) => Err(err),
    }
}

async fn update_dns_record(
    hetzner_config: &HetznerConfig,
    record: &DnsRecord,
) -> Result<DnsRecord, Error> {
    let client = Client::new();
    let url = format!("{}/records/{}", &hetzner_config.api_url, &record.id);
    let res = client
        .put(url)
        .header("Auth-API-Token", &hetzner_config.dns_token)
        .json(record)
        .send()
        .await?;

    let updated_dns = res.json::<RecordResponse>().await?;
    Ok(updated_dns.record)
}

async fn create_dns_record(
    hetzner_config: &HetznerConfig,
    record_create: &DnsRecordCreate,
) -> Result<DnsRecord, Error> {
    let client = Client::new();
    let url = format!("{}/records", &hetzner_config.api_url);
    let res = client
        .post(url)
        .header("Auth-API-Token", &hetzner_config.dns_token)
        .json(record_create)
        .send()
        .await?
        .error_for_status()?;

    let created_dns = res.json::<RecordResponse>().await?;
    Ok(created_dns.record)
}
