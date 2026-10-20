// Copyright © 2026 Tess Dev Team <https://tessheadless.com>
// SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
// Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

//! Copies the Phusion Passenger launcher (`deploy/passenger/config.ru` and its
//! `Gemfile`) next to the `tess` binary, e.g. `target/release/`, so the build
//! folder can be deployed to Passenger-only hosts as is (see
//! `docs/docs/devops/passenger.md`).

use std::path::{Path, PathBuf};

const FILES: &[&str] = &["config.ru", "Gemfile"];

fn main() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deploy/passenger");
    for f in FILES {
        println!("cargo:rerun-if-changed={}", src.join(f).display());
    }
    // OUT_DIR is `<target>/<profile>/build/<pkg>-<hash>/out`; binaries live in `<target>/<profile>`.
    let Some(bin_dir) = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .and_then(|o| o.ancestors().nth(3).map(Path::to_path_buf))
    else {
        return;
    };
    for f in FILES {
        let from = src.join(f);
        if !from.exists() {
            continue; // packaged sources without the deploy folder
        }
        if let Err(e) = std::fs::copy(&from, bin_dir.join(f)) {
            println!(
                "cargo:warning=cannot copy {} to {}: {e}",
                from.display(),
                bin_dir.display()
            );
        }
    }
}
