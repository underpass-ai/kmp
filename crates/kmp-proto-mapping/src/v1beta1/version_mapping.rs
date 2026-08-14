use std::time::SystemTime;

use kmp_application::AcceptedVersion;
use kmp_domain::BundleMetadata;
use kmp_proto::v1beta1::BundleVersion;

use crate::v1beta1::support::timestamp_from;

pub fn proto_accepted_version_v1beta1(version: &AcceptedVersion) -> BundleVersion {
    BundleVersion {
        revision: version.revision,
        content_hash: version.content_hash.clone(),
        schema_version: "v1beta1".to_string(),
        projection_watermark: format!("rev-{}", version.revision),
        generated_at: Some(timestamp_from(SystemTime::now())),
        generator_version: version.generator_version.clone(),
    }
}

pub fn proto_bundle_version_v1beta1(metadata: &BundleMetadata) -> BundleVersion {
    BundleVersion {
        revision: metadata.revision,
        content_hash: metadata.content_hash.clone(),
        schema_version: "v1beta1".to_string(),
        projection_watermark: format!("rev-{}", metadata.revision),
        generated_at: Some(timestamp_from(SystemTime::now())),
        generator_version: metadata.generator_version.clone(),
    }
}
