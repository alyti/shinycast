use std::collections::HashMap;

use async_graphql::{
    scalar, ComplexObject, Context, EmptySubscription, InputObject, Object, Schema, SimpleObject,
};

use sled::Db;

use clokwerk::{Interval::Minutes, Job, AsyncScheduler, timeprovider::TimeProvider};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use thiserror::Error as ThisError;

pub type PodcastSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;

#[derive(Eq, PartialEq, Debug, Copy, Clone, Serialize, Deserialize)]
pub struct Interval(pub clokwerk::Interval);

scalar!(
    Interval,
    "interval",
    "recurrance interval",
    "https://github.com/alyti/clokwerk/blob/serde/src/intervals.rs#L7"
);

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Adjustment {
    At(chrono::NaiveTime),
    Plus(Interval),
    AndEvery(Interval),
    Count(usize),
    RepeatingEvery(Interval, usize),
}

scalar!(
    Adjustment,
    "adjustment",
    "sub-intervals or specific time",
    "https://github.com/alyti/clokwerk/blob/serde/src/job.rs#L59"
);

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize, InputObject)]
pub struct ScheduleConfigurationProposal {
    base: Interval,
    adjustment: Option<Vec<Adjustment>>,
}

impl Into<ScheduleConfiguration> for ScheduleConfigurationProposal {
    fn into(self) -> ScheduleConfiguration {
        ScheduleConfiguration {
            base: self.base,
            adjustment: self.adjustment,
        }
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize, SimpleObject)]
pub struct ScheduleConfiguration {
    pub base: Interval,
    pub adjustment: Option<Vec<Adjustment>>,
}

pub trait ConfigSchedulerExt<Tz, Tp>
where
    Tz: chrono::TimeZone,
    Tp: TimeProvider {
    fn new_job_from_config(&mut self, c: &ScheduleConfiguration) -> &mut clokwerk::AsyncJob<Tz, Tp>;
}

impl<Tz, Tp> ConfigSchedulerExt<Tz, Tp> for AsyncScheduler<Tz, Tp>
where
    Tz: chrono::TimeZone + Sync + Send,
    Tp: TimeProvider {
    fn new_job_from_config(&mut self, c: &ScheduleConfiguration) -> &mut clokwerk::AsyncJob<Tz, Tp> {
        let mut job = self.every(c.base.0);
        if let Some(adjustments) = &c.adjustment {
            for adj in adjustments {
                match adj {
                    Adjustment::At(x) => job = job.at_time(x.clone()),
                    Adjustment::Plus(x) => job = job.plus(x.0.clone()),
                    Adjustment::AndEvery(x) => job = job.and_every(x.0.clone()),
                    Adjustment::Count(x) => job = job.count(x.clone()),
                    Adjustment::RepeatingEvery(x, y) => {
                        job = job.repeating_every(x.0.clone()).times(y.clone())
                    }
                }
            }
        }
        return job;
    }
}

#[derive(Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub enum Source {
    /// YouTube source, SponsorBlock supported.
    // https://www.youtube.com/feeds/videos.xml?channel_id=
    Youtube(String),
    // sponsorblock only has data on youtube for now so its kinda pointless to add anything else for now...
}
scalar!(Source);

#[derive(Clone, Debug, Serialize, Deserialize, SimpleObject)]
#[graphql(complex)]
// Stored podcast entry
pub struct Podcast {
    /// Podcast name and the main identifier
    pub name: String,
    /// Where feed is originally sourced from
    pub source: Source,
    /// How often should we check the source? By default never.
    pub update_schedule: Option<ScheduleConfiguration>,
    /// Do we remove any sponsorblock segments? By default no.
    pub sponsorblock_categories: Option<Vec<String>>,
    /// Any special secret flags?
    pub downloader_arguments: Option<Vec<String>>,
}

#[ComplexObject]
impl Podcast {
    /// Episodes (query-todo)
    async fn episodes(&self) -> Option<Vec<bool>> {
        None
    }

    /// URL to Feed for the podcast, if it's empty it hasn't been processed yet.
    async fn feed(&self, _ctx: &Context<'_>) -> Option<url::Url> {
        None
    }
}

lazy_static! {
    static ref SPONSORBLOCK_CATEGORIES: HashMap<&'static str, &'static str> = vec![
        ("sponsor", "Sponsor"),
        ("intro", "Intermission/Intro Animation"),
        ("outro", "Endcards/Credits"),
        ("selfpromo", "Unpaid/Self Promotion"),
        ("interaction", "Interaction Reminder"),
        ("preview", "Preview/Recap"),
        ("music_offtopic", "Non-Music Section")
    ]
    .into_iter()
    .collect();
}

#[derive(Clone, Debug, Serialize, Deserialize, SimpleObject)]
/// Server configuration, some options might require a restart to take effect.
pub struct ServerConfig {
    /// How often should worker responsible for downloading process queue.
    pub downloader_schedule: ScheduleConfiguration,
    /// Where all media and feed are placed (and served from if enabled).
    pub media_directory: String,
    /// Should the server also serve feeds themselves? By default no.
    /// If enabled this will provide /:podcast/feed & /:podcast/media/:id.ext routes.
    pub serve_feed_and_media: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            downloader_schedule: ScheduleConfiguration {
                base: Interval(Minutes(5)),
                adjustment: None,
            },
            media_directory: "media".to_owned(),
            serve_feed_and_media: false,
        }
    }
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    /// List of all podcasts
    async fn podcasts(&self, ctx: &Context<'_>) -> Vec<Podcast> {
        let storage = ctx
            .data_unchecked::<Db>()
            .open_tree("podcasts")
            .expect("cant open podcasts tree");
        storage
            .iter()
            .filter_map(|r| r.ok())
            .filter_map(|(_, p)| serde_json::from_slice(&p).ok())
            .collect()
    }

    /// Server config
    async fn server_config(&self, ctx: &Context<'_>) -> Result<ServerConfig, Error> {
        let config = ctx
            .data_unchecked::<Db>()
            .get("config")?;

        match config {
            Some(v) => Ok(serde_json::from_slice(&v)?),
            None => Ok(ServerConfig::default()),
        }
    }

    /// Map of allowed sponsorblock categories
    async fn allowed_sponsorblock_categories(&self) -> &HashMap<&str, &str> {
        &SPONSORBLOCK_CATEGORIES
    }
}

/// Wrapper error types
#[derive(ThisError, Debug)]
pub enum Error {
    #[error("data store failed")]
    Storage(#[from] sled::Error),

    #[error("serialization failed")]
    Serialization(#[from] serde_json::Error),

    #[error("requested podcast does not exist")]
    PodcastNotFound,

    #[error("server config is missing")]
    ConfigNotFound,
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Create a podcast using provided source, stored using name as key.
    /// If podcast already exists it will be overwritten.
    async fn create_podcast(
        &self,
        ctx: &Context<'_>,
        name: String,
        source: Source,
        schedule_configuration: Option<ScheduleConfigurationProposal>,
        sponsorblock_categories: Option<Vec<String>>,
        downloader_arguments: Option<Vec<String>>,
    ) -> Result<bool, Error> {
        let storage = ctx
            .data_unchecked::<Db>()
            .open_tree("podcasts")
            .expect("cant open podcasts tree");

        let podcast = Podcast {
            source: source,
            name: name.clone(),
            update_schedule: match schedule_configuration {
                // TODO: Set some sane limits?
                Some(x) => Some(x.into()),
                None => None,
            },
            sponsorblock_categories: match sponsorblock_categories {
                Some(cats) => {
                    if cats.len() == 1 && cats[0] == "all" {
                        // `all` is a special case :)
                        Some(
                            SPONSORBLOCK_CATEGORIES
                                .keys()
                                .map(|x| x.to_string())
                                .collect(),
                        )
                    } else {
                        // Filter requested categories for only supported ones.
                        Some(
                            cats.iter()
                                .filter(|x| SPONSORBLOCK_CATEGORIES.contains_key(x.as_str()))
                                .cloned()
                                .collect(),
                        )
                    }
                }
                None => None,
            },
            downloader_arguments,
        };
        storage.insert(name, serde_json::to_vec_pretty(&podcast)?)?;
        storage.flush_async().await?;
        Ok(true)
    }

    /// Delete podcast and any related media.
    async fn purge_podcast(&self, _ctx: &Context<'_>, _name: String) -> Result<bool, Error> {
        Ok(false)
    }

    /// Bypass job scheduler and manually start processing of a podcast.
    async fn manually_process_podcast(
        &self,
        ctx: &Context<'_>,
        name: String,
        _overwrite_existing: bool,
    ) -> Result<bool, Error> {
        let storage = ctx
            .data_unchecked::<Db>()
            .open_tree("podcasts")
            .expect("cant open podcasts tree");

        if let Some(v) = storage.get(name)? {
            let podcast: Podcast = serde_json::from_slice(&v)?;
            println!("{:?}", podcast);
            Ok(true)
        } else {
            Err(Error::PodcastNotFound)
        }
    }
}
