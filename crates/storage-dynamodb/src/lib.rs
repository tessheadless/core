// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! DynamoDB backend (single-table design), the natural fit for AWS Lambda.
//!
//! | pk            | sk     | doc                 |
//! |---------------|--------|---------------------|
//! | `CT`          | apiId  | content type JSON   |
//! | `E#{apiId}`   | uuid   | entry JSON          |
//!
//! Listing a content type is a single-partition `Query`; filters, sorting and
//! pagination are then evaluated in memory. That is fine for the content
//! volumes a CMS typically has (thousands of entries per type), but each
//! listing reads the whole partition: put a cache/CDN in front for heavy read
//! traffic. Items are limited to 400 KB by DynamoDB.

use std::collections::HashMap;

use async_trait::async_trait;
use aws_sdk_dynamodb::Client;
use aws_sdk_dynamodb::types::{
    AttributeDefinition, AttributeValue, BillingMode, DeleteRequest, KeySchemaElement, KeyType,
    ScalarAttributeType, TableStatus, WriteRequest,
};
use tess_core::query::execute_in_memory;
use tess_core::{CmsError, ContentType, Entry, EntryQuery, Page, Result, Storage};
use uuid::Uuid;

const CT_PK: &str = "CT";
const SETTINGS_PK: &str = "S";

pub struct DynamoDbStorage {
    client: Client,
    table: String,
    create_table: bool,
}

impl DynamoDbStorage {
    /// `endpoint` overrides the AWS endpoint (DynamoDB Local, LocalStack).
    /// With `create_table`, [`Storage::migrate`] creates the table if it is
    /// missing; in AWS the table is normally provisioned by IaC instead.
    pub async fn connect(table: &str, endpoint: Option<&str>, create_table: bool) -> Self {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(endpoint) = endpoint {
            loader = loader.endpoint_url(endpoint);
        }
        let config = loader.load().await;
        DynamoDbStorage { client: Client::new(&config), table: table.to_string(), create_table }
    }

    async fn get_doc(&self, pk: String, sk: String) -> Result<Option<String>> {
        let out = self
            .client
            .get_item()
            .table_name(&self.table)
            .key("pk", AttributeValue::S(pk))
            .key("sk", AttributeValue::S(sk))
            .consistent_read(true)
            .send()
            .await
            .map_err(storage_err)?;
        Ok(out.item.as_ref().and_then(doc_attr))
    }

    async fn put_doc(&self, pk: String, sk: String, doc: String) -> Result<()> {
        self.client
            .put_item()
            .table_name(&self.table)
            .item("pk", AttributeValue::S(pk))
            .item("sk", AttributeValue::S(sk))
            .item("doc", AttributeValue::S(doc))
            .send()
            .await
            .map_err(storage_err)?;
        Ok(())
    }

    /// All items of a partition (follows pagination).
    async fn query_partition(&self, pk: &str) -> Result<Vec<HashMap<String, AttributeValue>>> {
        let mut items = Vec::new();
        let mut start: Option<HashMap<String, AttributeValue>> = None;
        loop {
            let out = self
                .client
                .query()
                .table_name(&self.table)
                .key_condition_expression("pk = :pk")
                .expression_attribute_values(":pk", AttributeValue::S(pk.to_string()))
                .consistent_read(true)
                .set_exclusive_start_key(start.take())
                .send()
                .await
                .map_err(storage_err)?;
            items.extend(out.items.unwrap_or_default());
            match out.last_evaluated_key {
                Some(key) if !key.is_empty() => start = Some(key),
                _ => return Ok(items),
            }
        }
    }

    async fn batch_delete(&self, keys: Vec<HashMap<String, AttributeValue>>) -> Result<()> {
        for chunk in keys.chunks(25) {
            let mut requests: Vec<WriteRequest> = chunk
                .iter()
                .map(|key| {
                    let delete = DeleteRequest::builder().set_key(Some(key.clone())).build().map_err(storage_err)?;
                    Ok(WriteRequest::builder().delete_request(delete).build())
                })
                .collect::<Result<_>>()?;
            let mut attempt = 0u32;
            while !requests.is_empty() {
                let out = self
                    .client
                    .batch_write_item()
                    .request_items(&self.table, requests)
                    .send()
                    .await
                    .map_err(storage_err)?;
                requests = out.unprocessed_items.unwrap_or_default().remove(&self.table).unwrap_or_default();
                if !requests.is_empty() {
                    attempt += 1;
                    if attempt > 8 {
                        return Err(CmsError::Storage("batch delete kept being throttled".into()));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(50 << attempt)).await;
                }
            }
        }
        Ok(())
    }

    async fn ensure_table(&self) -> Result<()> {
        match self.client.describe_table().table_name(&self.table).send().await {
            Ok(_) => return Ok(()),
            Err(err) if err.as_service_error().is_some_and(|e| e.is_resource_not_found_exception()) => {}
            Err(err) => return Err(storage_err(err)),
        }
        let attr = |name: &str| {
            AttributeDefinition::builder()
                .attribute_name(name)
                .attribute_type(ScalarAttributeType::S)
                .build()
                .map_err(storage_err)
        };
        let key = |name: &str, kind| {
            KeySchemaElement::builder().attribute_name(name).key_type(kind).build().map_err(storage_err)
        };
        let created = self
            .client
            .create_table()
            .table_name(&self.table)
            .attribute_definitions(attr("pk")?)
            .attribute_definitions(attr("sk")?)
            .key_schema(key("pk", KeyType::Hash)?)
            .key_schema(key("sk", KeyType::Range)?)
            .billing_mode(BillingMode::PayPerRequest)
            .send()
            .await;
        if let Err(err) = created {
            // Another instance may have won the race.
            if !err.as_service_error().is_some_and(|e| e.is_resource_in_use_exception()) {
                return Err(storage_err(err));
            }
        }
        for _ in 0..60 {
            let out = self.client.describe_table().table_name(&self.table).send().await.map_err(storage_err)?;
            if out.table.and_then(|t| t.table_status) == Some(TableStatus::Active) {
                return Ok(());
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(CmsError::Storage(format!("table `{}` did not become active", self.table)))
    }
}

fn storage_err(err: impl std::error::Error) -> CmsError {
    // The SDK's Display is terse; include the source chain.
    let mut msg = err.to_string();
    let mut source = err.source();
    while let Some(s) = source {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        source = s.source();
    }
    CmsError::Storage(msg)
}

fn doc_attr(item: &HashMap<String, AttributeValue>) -> Option<String> {
    item.get("doc").and_then(|v| v.as_s().ok()).cloned()
}

fn entries_pk(api_id: &str) -> String {
    format!("E#{api_id}")
}

fn decode<T: serde::de::DeserializeOwned>(doc: &str) -> Result<T> {
    serde_json::from_str(doc).map_err(CmsError::storage)
}

fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(CmsError::storage)
}

#[async_trait]
impl Storage for DynamoDbStorage {
    fn name(&self) -> &'static str {
        "dynamodb"
    }

    async fn migrate(&self) -> Result<()> {
        if self.create_table {
            self.ensure_table().await?;
        }
        Ok(())
    }

    async fn list_content_types(&self) -> Result<Vec<ContentType>> {
        self.query_partition(CT_PK)
            .await?
            .iter()
            .filter_map(doc_attr)
            .map(|d| decode(&d))
            .collect()
    }

    async fn get_content_type(&self, api_id: &str) -> Result<Option<ContentType>> {
        self.get_doc(CT_PK.into(), api_id.into()).await?.map(|d| decode(&d)).transpose()
    }

    async fn put_content_type(&self, content_type: &ContentType) -> Result<()> {
        self.put_doc(CT_PK.into(), content_type.api_id.clone(), encode(content_type)?).await
    }

    async fn delete_content_type(&self, api_id: &str) -> Result<()> {
        let key = |pk: String, sk: String| {
            HashMap::from([("pk".to_string(), AttributeValue::S(pk)), ("sk".to_string(), AttributeValue::S(sk))])
        };
        let mut keys: Vec<_> = self
            .query_partition(&entries_pk(api_id))
            .await?
            .into_iter()
            .filter_map(|item| {
                let sk = item.get("sk")?.as_s().ok()?.clone();
                Some(key(entries_pk(api_id), sk))
            })
            .collect();
        keys.push(key(CT_PK.into(), api_id.into()));
        self.batch_delete(keys).await
    }

    async fn get_entry(&self, content_type: &str, id: Uuid) -> Result<Option<Entry>> {
        self.get_doc(entries_pk(content_type), id.to_string()).await?.map(|d| decode(&d)).transpose()
    }

    async fn put_entry(&self, entry: &Entry) -> Result<()> {
        self.put_doc(entries_pk(&entry.content_type), entry.id.to_string(), encode(entry)?).await
    }

    async fn delete_entry(&self, content_type: &str, id: Uuid) -> Result<bool> {
        let out = self
            .client
            .delete_item()
            .table_name(&self.table)
            .key("pk", AttributeValue::S(entries_pk(content_type)))
            .key("sk", AttributeValue::S(id.to_string()))
            .return_values(aws_sdk_dynamodb::types::ReturnValue::AllOld)
            .send()
            .await
            .map_err(storage_err)?;
        Ok(out.attributes.is_some_and(|a| !a.is_empty()))
    }

    async fn query_entries(&self, content_type: &str, query: &EntryQuery) -> Result<Page> {
        let entries = self
            .query_partition(&entries_pk(content_type))
            .await?
            .iter()
            .filter_map(doc_attr)
            .map(|d| decode::<Entry>(&d))
            .collect::<Result<Vec<_>>>()?;
        Ok(execute_in_memory(entries, query))
    }

    async fn get_setting(&self, key: &str) -> Result<Option<serde_json::Value>> {
        self.get_doc(SETTINGS_PK.into(), key.into()).await?.map(|d| decode(&d)).transpose()
    }

    async fn put_setting(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        self.put_doc(SETTINGS_PK.into(), key.into(), encode(value)?).await
    }

    async fn delete_setting(&self, key: &str) -> Result<bool> {
        let out = self
            .client
            .delete_item()
            .table_name(&self.table)
            .key("pk", AttributeValue::S(SETTINGS_PK.into()))
            .key("sk", AttributeValue::S(key.into()))
            .return_values(aws_sdk_dynamodb::types::ReturnValue::AllOld)
            .send()
            .await
            .map_err(storage_err)?;
        Ok(out.attributes.is_some_and(|a| !a.is_empty()))
    }

    async fn list_settings(&self, prefix: &str) -> Result<Vec<(String, serde_json::Value)>> {
        // Sort keys come back in byte order.
        self.query_partition(SETTINGS_PK)
            .await?
            .iter()
            .filter_map(|item| {
                let key = item.get("sk")?.as_s().ok()?;
                key.starts_with(prefix).then(|| Some((key.clone(), doc_attr(item)?)))?
            })
            .map(|(k, d)| Ok((k, decode(&d)?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    /// Runs against DynamoDB Local at `TESS_TEST_DYNAMODB_ENDPOINT`; skipped when unset.
    #[tokio::test]
    async fn conformance() {
        let Ok(endpoint) = std::env::var("TESS_TEST_DYNAMODB_ENDPOINT") else {
            eprintln!("TESS_TEST_DYNAMODB_ENDPOINT not set, skipping");
            return;
        };
        let storage = DynamoDbStorage::connect("tess_conformance", Some(&endpoint), true).await;
        tess_core::testkit::run_conformance(Arc::new(storage)).await;
    }
}
