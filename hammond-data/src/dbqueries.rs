#![cfg_attr(feature = "cargo-clippy", allow(let_and_return))]

use diesel::prelude::*;
use models::{Episode, Podcast, Source};

// TODO: Needs cleanup.

pub fn get_sources(con: &SqliteConnection) -> QueryResult<Vec<Source>> {
    use schema::source::dsl::*;

    let s = source.load::<Source>(con);
    s
}

pub fn get_podcasts(con: &SqliteConnection) -> QueryResult<Vec<Podcast>> {
    use schema::podcast::dsl::*;

    let pds = podcast.load::<Podcast>(con);
    pds
}

// Maybe later.
// pub fn get_podcasts_ids(con: &SqliteConnection) -> QueryResult<Vec<i32>> {
//     use schema::podcast::dsl::*;

//     let pds = podcast.select(id).load::<i32>(con);
//     pds
// }

pub fn get_episodes(con: &SqliteConnection) -> QueryResult<Vec<Episode>> {
    use schema::episode::dsl::*;

    let eps = episode.order(epoch.desc()).load::<Episode>(con);
    eps
}

pub fn get_episodes_with_limit(con: &SqliteConnection, limit: u32) -> QueryResult<Vec<Episode>> {
    use schema::episode::dsl::*;

    let eps = episode
        .order(epoch.desc())
        .limit(i64::from(limit))
        .load::<Episode>(con);
    eps
}

pub fn get_podcast(con: &SqliteConnection, parent: &Source) -> QueryResult<Vec<Podcast>> {
    let pd = Podcast::belonging_to(parent).load::<Podcast>(con);
    // debug!("Returned Podcasts:\n{:?}", pds);
    pd
}

pub fn get_pd_episodes(con: &SqliteConnection, parent: &Podcast) -> QueryResult<Vec<Episode>> {
    use schema::episode::dsl::*;

    let eps = Episode::belonging_to(parent)
        .order(epoch.desc())
        .load::<Episode>(con);
    eps
}

pub fn get_pd_episodes_limit(
    con: &SqliteConnection,
    parent: &Podcast,
    limit: u32,
) -> QueryResult<Vec<Episode>> {
    use schema::episode::dsl::*;

    let eps = Episode::belonging_to(parent)
        .order(epoch.desc())
        .limit(i64::from(limit))
        .load::<Episode>(con);
    eps
}

pub fn load_source(con: &SqliteConnection, uri_: &str) -> QueryResult<Source> {
    use schema::source::dsl::*;

    let s = source.filter(uri.eq(uri_)).get_result::<Source>(con);
    s
}

pub fn load_podcast(con: &SqliteConnection, title_: &str) -> QueryResult<Podcast> {
    use schema::podcast::dsl::*;

    let pd = podcast.filter(title.eq(title_)).get_result::<Podcast>(con);
    pd
}

pub fn load_episode(con: &SqliteConnection, uri_: &str) -> QueryResult<Episode> {
    use schema::episode::dsl::*;

    let ep = episode.filter(uri.eq(uri_)).get_result::<Episode>(con);
    ep
}
