use diesel::SaveChangesDsl;
use rss::Channel;

use hyper;
use hyper::{Client, Method, Request, Response, StatusCode, Uri};
use hyper::client::HttpConnector;
use hyper::header::{ETag, EntityTag, HttpDate, IfModifiedSince, IfNoneMatch, LastModified};
use hyper_tls::HttpsConnector;

use futures::future::ok;
use futures::prelude::*;

use database::connection;
use errors::*;
use feed::{Feed, FeedBuilder};
use models::NewSource;
use schema::source;

use std::str::FromStr;

#[derive(Queryable, Identifiable, AsChangeset, PartialEq)]
#[table_name = "source"]
#[changeset_options(treat_none_as_null = "true")]
#[derive(Debug, Clone)]
/// Diesel Model of the source table.
pub struct Source {
    id: i32,
    uri: String,
    last_modified: Option<String>,
    http_etag: Option<String>,
}

impl Source {
    /// Get the source `id` column.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// Represents the location(usually url) of the Feed xml file.
    pub fn uri(&self) -> &str {
        &self.uri
    }

    /// Represents the Http Last-Modified Header field.
    ///
    /// See [RFC 7231](https://tools.ietf.org/html/rfc7231#section-7.2) for more.
    pub fn last_modified(&self) -> Option<&str> {
        self.last_modified.as_ref().map(|s| s.as_str())
    }

    /// Set `last_modified` value.
    pub fn set_last_modified(&mut self, value: Option<String>) {
        // self.last_modified = value.map(|x| x.to_string());
        self.last_modified = value;
    }

    /// Represents the Http Etag Header field.
    ///
    /// See [RFC 7231](https://tools.ietf.org/html/rfc7231#section-7.2) for more.
    pub fn http_etag(&self) -> Option<&str> {
        self.http_etag.as_ref().map(|s| s.as_str())
    }

    /// Set `http_etag` value.
    pub fn set_http_etag(&mut self, value: Option<&str>) {
        self.http_etag = value.map(|x| x.to_string());
    }

    /// Helper method to easily save/"sync" current state of self to the Database.
    pub fn save(&self) -> Result<Source> {
        let db = connection();
        let con = db.get()?;

        Ok(self.save_changes::<Source>(&*con)?)
    }

    /// Extract Etag and LastModifier from res, and update self and the
    /// corresponding db row.
    fn update_etag(&mut self, res: &Response) -> Result<()> {
        let headers = res.headers();

        let etag = headers.get::<ETag>().map(|x| x.tag());
        let lmod = headers.get::<LastModified>().map(|x| format!("{}", x));

        if (self.http_etag() != etag) || (self.last_modified != lmod) {
            self.set_http_etag(etag);
            self.set_last_modified(lmod);
            self.save()?;
        }

        Ok(())
    }

    /// Construct a new `Source` with the given `uri` and index it.
    ///
    /// This only indexes the `Source` struct, not the Podcast Feed.
    pub fn from_url(uri: &str) -> Result<Source> {
        NewSource::new(uri).into_source()
    }

    /// `Feed` constructor.
    ///
    /// Fetches the latest xml Feed.
    ///
    /// Updates the validator Http Headers.
    ///
    /// Consumes `self` and Returns the corresponding `Feed` Object.
    // TODO: Refactor into TryInto once it lands on stable.
    pub fn into_feed(
        mut self,
        client: &Client<HttpsConnector<HttpConnector>>,
        ignore_etags: bool,
    ) -> Box<Future<Item = Feed, Error = Error>> {
        let id = self.id();
        let feed = self.request_constructor(client, ignore_etags)
            .map_err(From::from)
            .and_then(move |res| -> Result<Response> {
                self.update_etag(&res)?;
                Ok(res)
            })
            .and_then(|res| -> Result<Response> {
                match_status(res.status())?;
                Ok(res)
            })
            .and_then(response_to_channel)
            .map(move |chan| {
                FeedBuilder::default()
                    .channel(chan)
                    .source_id(id)
                    .build()
                    .unwrap()
            });

        Box::new(feed)
    }

    // TODO: make ignore_etags an Enum for better ergonomics.
    // #bools_are_just_2variant_enmus
    fn request_constructor(
        &self,
        client: &Client<HttpsConnector<HttpConnector>>,
        ignore_etags: bool,
    ) -> Box<Future<Item = Response, Error = hyper::Error>> {
        // FIXME: remove unwrap somehow
        let uri = Uri::from_str(self.uri()).unwrap();
        let mut req = Request::new(Method::Get, uri);

        if !ignore_etags {
            if let Some(foo) = self.http_etag() {
                req.headers_mut().set(IfNoneMatch::Items(vec![
                    EntityTag::new(true, foo.to_owned()),
                ]));
            }

            if let Some(foo) = self.last_modified() {
                if let Ok(x) = foo.parse::<HttpDate>() {
                    req.headers_mut().set(IfModifiedSince(x));
                }
            }
        }

        let work = client.request(req).map_err(From::from);
        Box::new(work)
    }
}

fn response_to_channel(res: Response) -> Box<Future<Item = Channel, Error = Error>> {
    let chan = res.body()
        .concat2()
        .map(|x| x.into_iter())
        .map_err(From::from)
        .and_then(|iter| ok(iter.collect::<Vec<u8>>()))
        .and_then(|utf_8_bytes| ok(String::from_utf8_lossy(&utf_8_bytes).into_owned()))
        .and_then(|buf| Channel::from_str(&buf).map_err(From::from));

    Box::new(chan)
}

// TODO match on more stuff
// 301: Moved Permanently
// 304: Up to date Feed, checked with the Etag
// 307: Temporary redirect of the url
// 308: Permanent redirect of the url
// 401: Unathorized
// 403: Forbidden
// 408: Timeout
// 410: Feed deleted
fn match_status(code: StatusCode) -> Result<()> {
    match code {
        StatusCode::NotModified => bail!("304: skipping.."),
        StatusCode::TemporaryRedirect => debug!("307: Temporary Redirect."),
        // TODO: Change the source uri to the new one
        StatusCode::MovedPermanently | StatusCode::PermanentRedirect => {
            warn!("Feed was moved permanently.");
            bail!("308: Feed was moved permanently.")
        }
        StatusCode::Unauthorized => bail!("401: Unauthorized."),
        StatusCode::Forbidden => bail!("403: Forbidden."),
        StatusCode::NotFound => bail!("404: Not found."),
        StatusCode::RequestTimeout => bail!("408: Request Timeout."),
        StatusCode::Gone => bail!("410: Feed was deleted."),
        _ => info!("HTTP StatusCode: {}", code),
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_core::reactor::Core;

    use database::truncate_db;
    use utils::get_feed;

    #[test]
    fn test_into_feed() {
        truncate_db().unwrap();

        let mut core = Core::new().unwrap();
        let client = Client::configure()
            .connector(HttpsConnector::new(4, &core.handle()).unwrap())
            .build(&core.handle());

        let url = "https://web.archive.org/web/20180120083840if_/https://feeds.feedburner.\
                   com/InterceptedWithJeremyScahill";
        let source = Source::from_url(url).unwrap();
        let id = source.id();

        let feed = source.into_feed(&client, true);
        let feed = core.run(feed).unwrap();

        let expected = get_feed("tests/feeds/2018-01-20-Intercepted.xml", id);
        assert_eq!(expected, feed);
    }
}
