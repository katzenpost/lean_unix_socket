// SPDX-FileCopyrightText: Copyright (C) 2026 David Stainton
// SPDX-License-Identifier: AGPL-3.0-only

use std::process::Command;

/// `cargo test` links Lean's shared runtime; find it at run time too.
fn main() {
    let out = Command::new("lean")
        .arg("--print-prefix")
        .output()
        .expect("lean on PATH");
    let prefix = String::from_utf8(out.stdout).expect("UTF-8 path");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}/lib/lean", prefix.trim());
    println!("cargo:rerun-if-changed=build.rs");
}
