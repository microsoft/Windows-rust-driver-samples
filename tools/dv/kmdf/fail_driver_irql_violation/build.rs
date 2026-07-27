// Copyright (c) Microsoft Corporation
// License: MIT OR Apache-2.0

fn main() -> anyhow::Result<()> {
    Ok(wdk_build::configure_wdk_binary_build()?)
}
