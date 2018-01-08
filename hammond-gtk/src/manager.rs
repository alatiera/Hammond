// use hammond_data::Episode;
use hammond_data::dbqueries;
use hammond_downloader::downloader::get_episode;
use hammond_downloader::downloader::DownloadProgress;

use app::Action;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::sync::mpsc::Sender;
// use std::sync::atomic::AtomicUsize;
// use std::path::PathBuf;
use std::thread;

#[derive(Debug)]
pub struct Progress {
    total_bytes: u64,
    downloaded_bytes: u64,
}

impl Progress {
    pub fn get_fraction(&self) -> f64 {
        info!("Progress: {:?}", self);
        let ratio = self.downloaded_bytes as f64 / self.total_bytes as f64;

        if ratio >= 1.0 {
            return 1.0;
        };
        ratio
    }
}

impl Default for Progress {
    fn default() -> Self {
        Progress {
            total_bytes: 0,
            downloaded_bytes: 0,
        }
    }
}

impl DownloadProgress for Progress {
    fn set_downloaded(&mut self, downloaded: u64) {
        self.downloaded_bytes = downloaded
    }

    fn set_size(&mut self, bytes: u64) {
        self.total_bytes = bytes;
    }
}

lazy_static! {
    pub static ref ACTIVE_DOWNLOADS: Arc<RwLock<HashMap<i32, Arc<Mutex<Progress>>>>> = {
        Arc::new(RwLock::new(HashMap::new()))
    };
}

pub fn add(id: i32, directory: &str, sender: Sender<Action>) {
    // Create a new `Progress` struct to keep track of dl progress.
    let prog = Arc::new(Mutex::new(Progress::default()));

    {
        let mut m = ACTIVE_DOWNLOADS.write().unwrap();
        m.insert(id, prog.clone());
    }
    {
        let m = ACTIVE_DOWNLOADS.read().unwrap();
        info!("ACTIVE DOWNLOADS: {:#?}", m);
    }

    let dir = directory.to_owned();
    thread::spawn(move || {
        let episode = dbqueries::get_episode_from_rowid(id).unwrap();
        let e = get_episode(&mut episode.into(), dir.as_str(), Some(prog));
        if let Err(err) = e {
            error!("Error: {}", err);
        };

        {
            let mut m = ACTIVE_DOWNLOADS.write().unwrap();
            m.remove(&id);
        }

        sender.send(Action::RefreshEpisodesView).unwrap();
        sender.send(Action::RefreshWidget).unwrap();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use hammond_downloader::downloader;

    use diesel::Identifiable;

    use hammond_data::database;
    use hammond_data::feed::*;
    use hammond_data::{Episode, Source};
    use hammond_data::dbqueries;

    use std::path::Path;
    use std::{thread, time};
    use std::sync::mpsc::channel;

    #[test]
    // This test inserts an rss feed to your `XDG_DATA/hammond/hammond.db` so we make it explicit
    // to run it.
    #[ignore]
    // THIS IS NOT A RELIABLE TEST
    // Just quick sanity check
    fn test_start_dl() {
        let url = "http://www.newrustacean.com/feed.xml";

        // Create and index a source
        let source = Source::from_url(url).unwrap();
        // Copy it's id
        let sid = source.id().clone();

        // Convert Source it into a Feed and index it
        let feed = source.into_feed(true).unwrap();
        index(&feed);

        // Get the Podcast
        let pd = dbqueries::get_podcast_from_source_id(sid).unwrap();
        // Get an episode
        let episode: Episode = {
            let con = database::connection();
            dbqueries::get_episode_from_pk(&*con.get().unwrap(), "e000: Hello, world!", *pd.id())
                .unwrap()
        };

        let (sender, _rx) = channel();

        let download_fold = downloader::get_download_folder(&pd.title()).unwrap();
        add(episode.rowid(), download_fold.as_str(), sender);

        // Give it soem time to download the file
        thread::sleep(time::Duration::from_secs(40));

        let final_path = format!("{}/{}.unknown", &download_fold, episode.rowid());
        println!("{}", &final_path);
        assert!(Path::new(&final_path).exists());
    }
}
