use glib;
use gtk;
use gtk::prelude::*;

use failure::Error;
use humansize::FileSize;
use open;
use take_mut;

use hammond_data::dbqueries;
use hammond_data::utils::get_download_folder;
use hammond_data::EpisodeWidgetQuery;

use app::Action;
use manager;
use widgets::episode_states::*;

use std::cell::RefCell;
use std::ops::DerefMut;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub struct EpisodeWidget {
    pub container: gtk::Box,
    date: DateMachine,
    duration: DurationMachine,
    title: Rc<RefCell<TitleMachine>>,
    media: Rc<RefCell<MediaMachine>>,
}

impl Default for EpisodeWidget {
    #[inline]
    fn default() -> Self {
        let builder = gtk::Builder::new_from_resource("/org/gnome/hammond/gtk/episode_widget.ui");

        let container: gtk::Box = builder.get_object("episode_container").unwrap();
        let progress: gtk::ProgressBar = builder.get_object("progress_bar").unwrap();

        let download: gtk::Button = builder.get_object("download_button").unwrap();
        let play: gtk::Button = builder.get_object("play_button").unwrap();
        let cancel: gtk::Button = builder.get_object("cancel_button").unwrap();

        let title: gtk::Label = builder.get_object("title_label").unwrap();
        let date: gtk::Label = builder.get_object("date_label").unwrap();
        let duration: gtk::Label = builder.get_object("duration_label").unwrap();
        let local_size: gtk::Label = builder.get_object("local_size").unwrap();
        let total_size: gtk::Label = builder.get_object("total_size").unwrap();

        let separator1: gtk::Label = builder.get_object("separator1").unwrap();
        let separator2: gtk::Label = builder.get_object("separator2").unwrap();
        let prog_separator: gtk::Label = builder.get_object("prog_separator").unwrap();

        let date_machine = DateMachine::new(date, 0);
        let dur_machine = DurationMachine::new(duration, separator1, None);
        let title_machine = Rc::new(RefCell::new(TitleMachine::new(title, false)));
        let media = MediaMachine::new(
            play,
            download,
            progress,
            cancel,
            total_size,
            local_size,
            separator2,
            prog_separator,
        );
        let media_machine = Rc::new(RefCell::new(media));

        EpisodeWidget {
            container,
            title: title_machine,
            duration: dur_machine,
            date: date_machine,
            media: media_machine,
        }
    }
}

impl EpisodeWidget {
    #[inline]
    pub fn new(episode: EpisodeWidgetQuery, sender: Sender<Action>) -> EpisodeWidget {
        let mut widget = EpisodeWidget::default();
        widget.init(episode, sender);
        widget
    }

    #[inline]
    fn init(&mut self, episode: EpisodeWidgetQuery, sender: Sender<Action>) {
        // Set the date label.
        self.set_date(episode.epoch());

        // Set the title label state.
        self.set_title(&episode);

        // Set the duaration label.
        self.set_duration(episode.duration());

        // Determine what the state of the media widgets should be.
        determine_media_state(self.media.clone(), &episode)
            .map_err(|err| error!("Error: {}", err))
            .map_err(|_| error!("Could not determine Media State"))
            .ok();

        let episode = Arc::new(Mutex::new(episode));
        self.connect_buttons(episode, sender);
    }

    #[inline]
    fn connect_buttons(&self, episode: Arc<Mutex<EpisodeWidgetQuery>>, sender: Sender<Action>) {
        let title = self.title.clone();
        if let Ok(media) = self.media.try_borrow_mut() {
            media.play_connect_clicked(clone!(episode, sender => move |_| {
                if let Ok(mut ep) = episode.lock() {
                    on_play_bttn_clicked(&mut ep, title.clone(), sender.clone())
                        .map_err(|err| error!("Error: {}", err))
                        .ok();
                }
            }));

            let media_machine = self.media.clone();
            media.download_connect_clicked(clone!(media_machine, episode, sender => move |dl| {
                // Make the button insensitive so it won't be pressed twice
                dl.set_sensitive(false);
                if let Ok(ep) = episode.lock() {
                    on_download_clicked(&ep, sender.clone())
                        .and_then(|_| {
                            info!("Donwload started succesfully.");
                            determine_media_state(media_machine.clone(), &ep)
                    })
                    .map_err(|err| error!("Error: {}", err))
                    .map_err(|_| error!("Could not determine Media State"))
                    .ok();
                }

                // Restore sensitivity after operations above complete
                dl.set_sensitive(true);
            }));
        }
    }

    #[inline]
    /// Determine the title state.
    fn set_title(&mut self, episode: &EpisodeWidgetQuery) {
        let mut machine = self.title.borrow_mut();
        machine.set_title(episode.title());
        take_mut::take(machine.deref_mut(), |title| {
            title.determine_state(episode.played().is_some())
        });
    }

    #[inline]
    /// Set the date label depending on the current time.
    fn set_date(&mut self, epoch: i32) {
        let machine = &mut self.date;
        take_mut::take(machine, |date| date.determine_state(i64::from(epoch)));
    }

    #[inline]
    /// Set the duration label.
    fn set_duration(&mut self, seconds: Option<i32>) {
        let machine = &mut self.duration;
        take_mut::take(machine, |duration| duration.determine_state(seconds));
    }
}

#[inline]
fn determine_media_state(
    media_machine: Rc<RefCell<MediaMachine>>,
    episode: &EpisodeWidgetQuery,
) -> Result<(), Error> {
    let id = episode.rowid();
    let active_dl = || -> Result<Option<_>, Error> {
        let m = manager::ACTIVE_DOWNLOADS
            .read()
            .map_err(|_| format_err!("Failed to get a lock on the mutex."))?;

        Ok(m.get(&id).cloned())
    }()?;

    let mut lock = media_machine.try_borrow_mut()?;
    take_mut::take(lock.deref_mut(), |media| {
        media.determine_state(
            episode.length(),
            active_dl.is_some(),
            episode.local_uri().is_some(),
        )
    });

    // Show or hide the play/delete/download buttons upon widget initialization.
    if let Some(prog) = active_dl {

        // set a callback that will update the state when the download finishes
        let id = episode.rowid();
        let callback = clone!(media_machine => move || {
            if let Ok(guard) = manager::ACTIVE_DOWNLOADS.read() {
                if !guard.contains_key(&id) {
                    if let Ok(ep) = dbqueries::get_episode_widget_from_rowid(id) {
                        determine_media_state(media_machine.clone(), &ep)
                            .map_err(|err| error!("Error: {}", err))
                            .map_err(|_| error!("Could not determine Media State"))
                            .ok();

                        return glib::Continue(false)
                    }
                }
            }

            glib::Continue(true)
        });
        gtk::timeout_add(250, callback);

        lock.cancel_connect_clicked(clone!(prog, media_machine => move |_| {
            if let Ok(mut m) = prog.lock() {
                m.cancel();
            }

            if let Ok(mut lock) = media_machine.try_borrow_mut() {
                if let Ok(episode) = dbqueries::get_episode_widget_from_rowid(id) {
                    take_mut::take(lock.deref_mut(), |media| {
                        media.determine_state(
                            episode.length(),
                            false,
                            episode.local_uri().is_some(),
                        )
                    });
                }
            }
        }));
        drop(lock);

        // Setup a callback that will update the progress bar.
        update_progressbar_callback(prog.clone(), media_machine.clone(), id);

        // Setup a callback that will update the total_size label
        // with the http ContentLength header number rather than
        // relying to the RSS feed.
        update_total_size_callback(prog.clone(), media_machine.clone());
    }

    Ok(())
}

#[inline]
fn on_download_clicked(
    ep: &EpisodeWidgetQuery,
    sender: Sender<Action>,
) -> Result<(), Error> {
    let pd = dbqueries::get_podcast_from_id(ep.podcast_id())?;
    let download_fold = get_download_folder(&pd.title())?;

    // Start a new download.
    manager::add(ep.rowid(), download_fold)?;

    // Update Views
    sender.send(Action::RefreshEpisodesViewBGR)?;

    Ok(())
}

#[inline]
fn on_play_bttn_clicked(
    episode: &mut EpisodeWidgetQuery,
    title: Rc<RefCell<TitleMachine>>,
    sender: Sender<Action>,
) -> Result<(), Error> {
    open_uri(episode.rowid())?;
    episode.set_played_now()?;

    let mut machine = title.try_borrow_mut()?;
    take_mut::take(machine.deref_mut(), |title| {
        title.determine_state(episode.played().is_some())
    });

    sender.send(Action::RefreshEpisodesViewBGR)?;
    Ok(())
}

#[inline]
fn open_uri(rowid: i32) -> Result<(), Error> {
    let uri = dbqueries::get_episode_local_uri_from_id(rowid)?
        .ok_or_else(|| format_err!("Expected Some found None."))?;

    if Path::new(&uri).exists() {
        info!("Opening {}", uri);
        open::that(&uri)?;
    } else {
        bail!("File \"{}\" does not exist.", uri);
    }

    Ok(())
}

// Setup a callback that will update the progress bar.
#[inline]
#[cfg_attr(feature = "cargo-clippy", allow(if_same_then_else))]
fn update_progressbar_callback(
    prog: Arc<Mutex<manager::Progress>>,
    media: Rc<RefCell<MediaMachine>>,
    episode_rowid: i32,
) {
    let callback = clone!(prog, media => move || {
        progress_bar_helper(prog.clone(), media.clone(), episode_rowid)
            .unwrap_or(glib::Continue(false))
    });
    timeout_add(300, callback);
}

#[inline]
#[allow(if_same_then_else)]
fn progress_bar_helper(
    prog: Arc<Mutex<manager::Progress>>,
    media: Rc<RefCell<MediaMachine>>,
    episode_rowid: i32,
) -> Result<glib::Continue, Error> {
    let (fraction, downloaded) = {
        let m = prog.lock()
            .map_err(|_| format_err!("Failed to get a lock on the mutex."))?;
        (m.get_fraction(), m.get_downloaded())
    };

    // I hate floating points.
    // Update the progress_bar.
    if (fraction >= 0.0) && (fraction <= 1.0) && (!fraction.is_nan()) {
        // Update local_size label
        let size = downloaded
            .file_size(SIZE_OPTS.clone())
            .map_err(|err| format_err!("{}", err))?;

        if let Ok(mut m) = media.try_borrow_mut() {
            m.update_progress(&size, fraction);
        }
    }

    // info!("Fraction: {}", progress_bar.get_fraction());
    // info!("Fraction: {}", fraction);

    // Check if the download is still active
    let active = {
        let m = manager::ACTIVE_DOWNLOADS
            .read()
            .map_err(|_| format_err!("Failed to get a lock on the mutex."))?;
        m.contains_key(&episode_rowid)
    };

    if (fraction >= 1.0) && (!fraction.is_nan()) {
        Ok(glib::Continue(false))
    } else if !active {
        Ok(glib::Continue(false))
    } else {
        Ok(glib::Continue(true))
    }
}

// Setup a callback that will update the total_size label
// with the http ContentLength header number rather than
// relying to the RSS feed.
#[inline]
fn update_total_size_callback(
    prog: Arc<Mutex<manager::Progress>>,
    media: Rc<RefCell<MediaMachine>>,
) {
    let callback = clone!(prog, media => move || {
        total_size_helper(prog.clone(), media.clone()).unwrap_or(glib::Continue(true))
    });
    timeout_add(500, callback);
}

#[inline]
fn total_size_helper(
    prog: Arc<Mutex<manager::Progress>>,
    media: Rc<RefCell<MediaMachine>>,
) -> Result<glib::Continue, Error> {
    // Get the total_bytes.
    let total_bytes = {
        let m = prog.lock()
            .map_err(|_| format_err!("Failed to get a lock on the mutex."))?;
        m.get_total_size()
    };

    debug!("Total Size: {}", total_bytes);
    if total_bytes != 0 {
        // Update the total_size label
        if let Ok(mut m) = media.try_borrow_mut() {
            take_mut::take(m.deref_mut(), |machine| {
                machine.set_size(Some(total_bytes as i32))
            });
        }

        // Do not call again the callback
        Ok(glib::Continue(false))
    } else {
        Ok(glib::Continue(true))
    }
}

// fn on_delete_bttn_clicked(episode_id: i32) -> Result<(), Error> {
//     let mut ep = dbqueries::get_episode_from_rowid(episode_id)?.into();
//     delete_local_content(&mut ep).map_err(From::from).map(|_| ())
// }
