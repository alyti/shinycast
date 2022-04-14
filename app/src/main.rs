mod dl;
mod feed;

use std::time;

use axum::extract::Extension;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use hyper::{Method, Server};
use tower_http::cors::{CorsLayer, Origin};

use async_graphql::http::{playground_source, GraphQLPlaygroundConfig};
use async_graphql::{EmptySubscription, Schema};
use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use sled::Config;

use model::{ConfigSchedulerExt, MutationRoot, PodcastSchema, QueryRoot, ServerConfig};
use worker::CancellationToken;

async fn graphql_handler(schema: Extension<PodcastSchema>, req: GraphQLRequest) -> GraphQLResponse {
    schema.execute(req.0).await.into()
}

async fn graphql_playground() -> impl IntoResponse {
    Html(playground_source(GraphQLPlaygroundConfig::new("/")))
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let db = Config::new().use_compression(true).path("db.sled").open()?;

    let cancel_token_root = CancellationToken::new();
    let service_worker_db = db.clone();
    let mut service_worker = worker::Worker::new(
        move |s| {
            let mut config = ServerConfig::default();
            if let Some(stored_config) = service_worker_db.get("config")? {
                config = serde_json::from_slice(&stored_config)?;
            }
            s.new_job_from_config(&config.downloader_schedule)
                .run(move || async move {
                    println!(
                        "{:?} {:?}",
                        time::SystemTime::now(),
                        "download worker stuff"
                    )
                });

            Ok(())
        },
        cancel_token_root.clone(),
        chrono::Utc, // TODO: Decide if UTC should be the sole Tz we use, maybe making this an option is worth it, IDK.
    );
    service_worker.try_schedule()?;

    let podcast_worker_db = db.clone();
    let storage = podcast_worker_db
        .open_tree("podcasts")
        .expect("cant open podcasts tree");
    let podcast_worker_storage = storage.clone();
    let mut podcast_worker = worker::Worker::new(
        move |s| {
            let podcasts: Vec<model::Podcast> = podcast_worker_storage
                .iter()
                .filter_map(|r| r.ok())
                .filter_map(|(_, p)| serde_json::from_slice(&p).ok())
                .collect();

            for podcast in podcasts {
                let b = Box::new(podcast);
                if let Some(run) = &b.update_schedule {
                    s.new_job_from_config(run).run(move || {
                        let podcast = b.clone();
                        // TODO: Write actual download and feed update logic...
                        async move { println!("{:?} {:?}", time::SystemTime::now(), podcast.clone()) }
                    });
                }
            }
            Ok(())
        },
        cancel_token_root,
        chrono::Utc, // TODO: Decide if UTC should be the sole Tz we use, maybe making this an option is worth it, IDK.
    );
    podcast_worker.try_schedule()?;

    let mut sub = storage.watch_prefix("");
    tokio::spawn(async move {
        while let Some(_) = (&mut sub).await {
            podcast_worker
                .try_schedule()
                .expect("failed to reschedule worker");
        }
    });

    let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(db)
        .finish();

    println!("Playground: http://localhost:8000");

    let app = Router::new()
        .route("/", get(graphql_playground).post(graphql_handler))
        .layer(Extension(schema))
        .layer(
            CorsLayer::new()
                .allow_origin(Origin::predicate(|_, _| true))
                .allow_methods(vec![Method::GET, Method::POST]),
        );

    Server::bind(&"0.0.0.0:8000".parse().unwrap())
        .serve(app.into_make_service())
        .await?;
    Ok(())
}
