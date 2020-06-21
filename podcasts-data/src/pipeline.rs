// pipeline.rs
//
// Copyright 2017 Jordan Petridis <jpetridis@gnome.org>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: GPL-3.0-or-later

// FIXME:
//! Docs.

use futures::{future::ok, lazy, prelude::*, stream::FuturesUnordered};
use tokio;

use hyper::client::HttpConnector;
use hyper::{Body, Client};
use hyper_tls::HttpsConnector;

use num_cpus;

use crate::errors::DataError;
use crate::Source;

use std::iter::FromIterator;

type HttpsClient = Client<HttpsConnector<HttpConnector>>;

/// The pipline to be run for indexing and updating a Podcast feed that originates from
/// `Source.uri`.
///
/// Messy temp diagram:
/// Source -> GET Request -> Update Etags -> Check Status -> Parse `xml/Rss` ->
/// Convert `rss::Channel` into `Feed` -> Index Podcast -> Index Episodes.
pub fn pipeline<'a, S>(sources: S, client: HttpsClient) -> impl Future<Item = (), Error = ()> + 'a
where
    S: Stream<Item = Source, Error = DataError> + Send + 'a,
{
    sources
        .and_then(move |s| s.into_feed(client.clone()))
        .map_err(|err| {
            match err {
                // Avoid spamming the stderr when its not an eactual error
                DataError::FeedNotModified(_) => (),
                _ => error!("Error: {}", err),
            }
        })
        .and_then(move |feed| {
            let fut = lazy(|| feed.index().map_err(|err| error!("Error: {}", err)));
            tokio::spawn(fut);
            Ok(())
        })
        // For each terminates the stream at the first error so we make sure
        // we pass good values regardless
        .then(move |_| ok(()))
        // Convert the stream into a Future to later execute as a tokio task
        .for_each(move |_| ok(()))
}

/// Creates a tokio `reactor::Core`, and a `hyper::Client` and
/// runs the pipeline to completion. The `reactor::Core` is dropped afterwards.
pub fn run<S>(sources: S) -> Result<(), DataError>
where
    S: IntoIterator<Item = Source>,
{
    let https = HttpsConnector::new(num_cpus::get())?;
    let client = Client::builder().build::<_, Body>(https);

    let foo = sources.into_iter().map(ok::<_, _>);
    let stream = FuturesUnordered::from_iter(foo);
    let p = pipeline(stream, client);
    tokio::run(p);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::truncate_db;
    use crate::dbqueries;
    use crate::Source;
    use failure::Error;

    // (path, url) tuples.
    const URLS: &[&str] = &[
        "https://web.archive.org/web/20180120083840if_/https://feeds.feedburner.\
         com/InterceptedWithJeremyScahill",
        "https://web.archive.org/web/20180120110314if_/https://feeds.feedburner.com/linuxunplugged",
        "https://web.archive.org/web/20180120110727if_/https://rss.acast.com/thetipoff",
        "https://web.archive.org/web/20180120104957if_/https://rss.art19.com/steal-the-stars",
        "https://web.archive.org/web/20180120104741if_/https://www.greaterthancode.\
         com/feed/podcast",
    ];

    #[test]
    /// Insert feeds and update/index them.
    fn test_pipeline() -> Result<(), Error> {
        truncate_db()?;
        let bad_url = "https://gitlab.gnome.org/World/podcasts.atom";
        // if a stream returns error/None it stops
        // bad we want to parse all feeds regardless if one fails
        Source::from_url(bad_url)?;

        URLS.iter().for_each(|url| {
            // Index the urls into the source table.
            Source::from_url(url).unwrap();
        });

        let sources = dbqueries::get_sources()?;
        run(sources)?;

        let sources = dbqueries::get_sources()?;
        // Run again to cover Unique constrains erros.
        run(sources)?;

        // Assert the index rows equal the controlled results
        assert_eq!(dbqueries::get_sources()?.len(), 6);
        assert_eq!(dbqueries::get_podcasts()?.len(), 5);
        assert_eq!(dbqueries::get_episodes()?.len(), 354);
        Ok(())
    }
}
