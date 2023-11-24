/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use aws_config::BehaviorVersion;
use tokio::select;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use polyhedron::app;


#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let shared_config = aws_config::load_defaults(BehaviorVersion::latest()).await;

    select! {
        res = app(&shared_config) => res,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Shutting down");
            Ok(())
        },
    }
}
