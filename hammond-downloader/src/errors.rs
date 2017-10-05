use reqwest;
use rss;
use diesel::result;
use hammond_data;

use std::io;

error_chain! {
    foreign_links {
        ReqError(reqwest::Error);
        IoError(io::Error);
        RSSError(rss::Error);
        DieselResultError(result::Error);
        HamDBError(hammond_data::errors::Error);
    }
}