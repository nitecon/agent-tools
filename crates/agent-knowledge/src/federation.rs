//! Deterministic, authorization-preserving merge for local and gateway reads.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederatedResource {
    pub canonical_uri: String,
    pub authority: String,
    pub title: String,
    pub content_hash: String,
    #[serde(default = "authorized_default")]
    pub authorized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederationBatch {
    pub gateway: String,
    #[serde(default)]
    pub results: Vec<FederatedResource>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederatedGroup {
    pub canonical_uri: String,
    pub title: String,
    pub content_hash: String,
    pub content_group: String,
    pub origins: Vec<FederatedOrigin>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct FederatedOrigin {
    pub gateway: String,
    pub authority: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederationFailure {
    pub gateway: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FederationResult {
    pub resources: Vec<FederatedGroup>,
    pub failures: Vec<FederationFailure>,
}

pub fn merge_batches(batches: impl IntoIterator<Item = FederationBatch>) -> FederationResult {
    let mut groups = BTreeMap::<String, FederatedGroup>::new();
    let mut failures = Vec::new();
    for mut batch in batches {
        if let Some(message) = batch.error.take() {
            failures.push(FederationFailure {
                gateway: batch.gateway,
                message,
                retryable: batch.retryable,
            });
            continue;
        }
        batch.results.sort_by(|left, right| {
            left.canonical_uri
                .cmp(&right.canonical_uri)
                .then_with(|| left.authority.cmp(&right.authority))
        });
        for resource in batch.results.into_iter().filter(|item| item.authorized) {
            let origin = FederatedOrigin {
                gateway: batch.gateway.clone(),
                authority: resource.authority,
            };
            groups
                .entry(resource.canonical_uri.clone())
                .and_modify(|group| {
                    if !group.origins.contains(&origin) {
                        group.origins.push(origin.clone());
                        group.origins.sort();
                    }
                })
                .or_insert_with(|| FederatedGroup {
                    canonical_uri: resource.canonical_uri,
                    title: resource.title,
                    content_group: resource.content_hash.clone(),
                    content_hash: resource.content_hash,
                    origins: vec![origin],
                });
        }
    }
    failures.sort_by(|left, right| left.gateway.cmp(&right.gateway));
    FederationResult {
        resources: groups.into_values().collect(),
        failures,
    }
}

fn authorized_default() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> FederationBatch {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../agent-cli/tests/fixtures/knowledge_graph/gateways")
            .join(name);
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn merge_deduplicates_identity_preserves_origins_and_labels_partial_failure() {
        let result = merge_batches([
            fixture("default.json"),
            fixture("additional.json"),
            fixture("failure.json"),
        ]);
        assert_eq!(
            result.resources.len(),
            1,
            "unauthorized records are excluded"
        );
        assert_eq!(result.resources[0].origins.len(), 2);
        assert_eq!(result.resources[0].content_group, "shared-checkout-hash");
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].gateway, "unavailable");
        assert!(result.failures[0].retryable);
    }

    #[test]
    fn equal_hashes_with_distinct_identities_remain_distinct_groups() {
        let result = merge_batches([FederationBatch {
            gateway: "default".to_owned(),
            results: vec![
                FederatedResource {
                    canonical_uri: "okf://a".to_owned(),
                    authority: "repository".to_owned(),
                    title: "A".to_owned(),
                    content_hash: "same".to_owned(),
                    authorized: true,
                },
                FederatedResource {
                    canonical_uri: "okf://b".to_owned(),
                    authority: "gateway".to_owned(),
                    title: "B".to_owned(),
                    content_hash: "same".to_owned(),
                    authorized: true,
                },
            ],
            error: None,
            retryable: false,
        }]);
        assert_eq!(result.resources.len(), 2);
        assert_eq!(
            result.resources[0].content_group,
            result.resources[1].content_group
        );
    }
}
