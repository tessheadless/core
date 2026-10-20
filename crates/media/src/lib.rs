// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Media library: binary storage ([`BlobStore`]: S3-compatible services such
//! as Garage, MinIO or AWS S3, or the local filesystem) and the built-in
//! `Asset` content type describing uploaded files.

mod blob;
mod fs;
mod media;
#[cfg(feature = "s3")]
mod s3;

pub use blob::{BlobStore, ObjectInfo, PresignedRequest};
pub use fs::FsBlobStore;
pub use media::{ASSET_TYPE, MediaConfig, MediaHook, MediaService, asset_definition};
#[cfg(feature = "s3")]
pub use s3::{S3BlobStore, S3Config};
