// app.rs
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


#![allow(new_without_default)]

use gio::{self, prelude::*, ActionMapExt, SettingsExt};
use glib::{self, Variant};
use gtk;
use gtk::prelude::*;

use gettextrs::{bindtextdomain, setlocale, textdomain, LocaleCategory};

use crossbeam_channel::{unbounded, Receiver, Sender};
use fragile::Fragile;
use podcasts_data::Show;

use crate::headerbar::Header;
use crate::settings::{self, WindowGeometry};
use crate::stacks::{Content, PopulatedState};
use crate::utils;
use crate::widgets::about_dialog;
use crate::widgets::appnotif::{InAppNotification, SpinnerState, State};
use crate::widgets::player;
use crate::widgets::show_menu::{mark_all_notif, remove_show_notif, ShowMenu};

use std::cell::RefCell;
use std::env;
use std::rc::Rc;
use std::sync::Arc;

use crate::config::{APP_ID, LOCALEDIR};
use crate::i18n::i18n;

/// Creates an action named `name` in the action map `T with the handler `F`
fn action<T, F>(thing: &T, name: &str, action: F)
where
    T: ActionMapExt,
    for<'r, 's> F: Fn(&'r gio::SimpleAction, &'s Option<Variant>) + 'static,
{
    // Create a stateless, parameterless action
    let act = gio::SimpleAction::new(name, None);
    // Connect the handler
    act.connect_activate(action);
    // Add it to the map
    thing.add_action(&act);
}

#[derive(Debug, Clone)]
pub(crate) enum Action {
    RefreshAllViews,
    RefreshEpisodesView,
    RefreshEpisodesViewBGR,
    RefreshShowsView,
    ReplaceWidget(Arc<Show>),
    RefreshWidgetIfSame(i32),
    ShowWidgetAnimated,
    ShowShowsAnimated,
    HeaderBarShowTile(String),
    HeaderBarNormal,
    MarkAllPlayerNotification(Arc<Show>),
    ShowUpdateNotif(Receiver<bool>),
    RemoveShow(Arc<Show>),
    ErrorNotification(String),
    InitEpisode(i32),
    InitShowMenu(Fragile<ShowMenu>),
    EmptyState,
    PopulatedState,
    RaiseWindow,
}

#[derive(Debug, Clone)]
pub(crate) struct App {
    instance: gtk::Application,
    window: gtk::ApplicationWindow,
    overlay: gtk::Overlay,
    settings: gio::Settings,
    content: Rc<Content>,
    headerbar: Rc<Header>,
    player: player::PlayerWrapper,
    updater: RefCell<Option<InAppNotification>>,
    sender: Sender<Action>,
    receiver: Receiver<Action>,
}

impl App {
    pub(crate) fn new(application: &gtk::Application) -> Rc<Self> {
        let settings = gio::Settings::new("org.gnome.Podcasts");

        let (sender, receiver) = unbounded();

        let window = gtk::ApplicationWindow::new(application);
        window.set_title(&i18n("Podcasts"));
        if APP_ID.ends_with("Devel") {
            window.get_style_context().add_class("devel");
        }

        let weak_s = settings.downgrade();
        let weak_app = application.downgrade();
        window.connect_delete_event(move |window, _| {
            let app = match weak_app.upgrade() {
                Some(a) => a,
                None => return Inhibit(false),
            };

            let settings = match weak_s.upgrade() {
                Some(s) => s,
                None => return Inhibit(false),
            };

            info!("Saving window position");
            WindowGeometry::from_window(&window).write(&settings);

            info!("Application is exiting");
            app.quit();
            Inhibit(false)
        });

        // Create a content instance
        let content = Content::new(&sender).expect("Content initialization failed.");

        // Create the headerbar
        let header = Header::new(&content, &sender);
        // Add the Headerbar to the window.
        window.set_titlebar(&header.container);

        // Add the content main stack to the overlay.
        let overlay = gtk::Overlay::new();
        overlay.add(&content.get_stack());

        let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // Add the overlay to the main Box
        wrap.add(&overlay);

        let player = player::PlayerWrapper::new(&sender);
        // Add the player to the main Box
        wrap.add(&player.action_bar);

        let updater = RefCell::new(None);

        window.add(&wrap);

        let app = App {
            instance: application.clone(),
            window,
            settings,
            overlay,
            headerbar: header,
            content,
            player,
            updater,
            sender,
            receiver,
        };

        Rc::new(app)
    }

    fn init(app: &Rc<Self>) {
        let cleanup_date = settings::get_cleanup_date(&app.settings);
        // Garbage collect watched episodes from the disk
        utils::cleanup(cleanup_date);

        app.setup_gactions();
        app.setup_timed_callbacks();

        // Retrieve the previous window position and size.
        WindowGeometry::from_settings(&app.settings).apply(&app.window);

        // Setup the Action channel
        gtk::timeout_add(25, clone!(app => move || app.setup_action_channel()));
    }

    fn setup_timed_callbacks(&self) {
        self.setup_dark_theme();
        self.setup_refresh_on_startup();
        self.setup_auto_refresh();
    }

    fn setup_dark_theme(&self) {
        let gtk_settings = gtk::Settings::get_default().unwrap();
        self.settings.bind(
            "dark-theme",
            &gtk_settings,
            "gtk-application-prefer-dark-theme",
            gio::SettingsBindFlags::DEFAULT,
        );
    }

    fn setup_refresh_on_startup(&self) {
        // Update the feeds right after the Application is initialized.
        let sender = self.sender.clone();
        if self.settings.get_boolean("refresh-on-startup") {
            info!("Refresh on startup.");
            let s: Option<Vec<_>> = None;
            utils::refresh(s, sender.clone());
        }
    }

    fn setup_auto_refresh(&self) {
        let refresh_interval = settings::get_refresh_interval(&self.settings).num_seconds() as u32;
        info!("Auto-refresh every {:?} seconds.", refresh_interval);

        let sender = self.sender.clone();
        gtk::timeout_add_seconds(refresh_interval, move || {
            let s: Option<Vec<_>> = None;
            utils::refresh(s, sender.clone());

            glib::Continue(true)
        });
    }

    /// Define the `GAction`s.
    ///
    /// Used in menus and the keyboard shortcuts dialog.
    #[cfg_attr(rustfmt, rustfmt_skip)]
    fn setup_gactions(&self) {
        let sender = &self.sender;
        let weak_win = self.window.downgrade();

        // Create the `refresh` action.
        //
        // This will trigger a refresh of all the shows in the database.
        action(&self.window, "refresh", clone!(sender => move |_, _| {
            gtk::idle_add(clone!(sender => move || {
                let s: Option<Vec<_>> = None;
                utils::refresh(s, sender.clone());
                glib::Continue(false)
            }));
        }));
        self.instance.set_accels_for_action("win.refresh", &["<primary>r"]);

        // Create the `OPML` import action
        action(&self.window, "import", clone!(sender, weak_win => move |_, _| {
            weak_win.upgrade().map(|win| utils::on_import_clicked(&win, &sender));
        }));

        action(&self.window, "export", clone!(sender, weak_win => move |_, _| {
            weak_win.upgrade().map(|win| utils::on_export_clicked(&win, &sender));
        }));

        // Create the action that shows a `gtk::AboutDialog`
        action(&self.window, "about", clone!(weak_win => move |_, _| {
            weak_win.upgrade().map(|win| about_dialog(&win));
        }));

        // Create the quit action
        let weak_instance = self.instance.downgrade();
        action(&self.window, "quit", move |_, _| {
            weak_instance.upgrade().map(|app| app.quit());
        });
        self.instance.set_accels_for_action("win.quit", &["<primary>q"]);

        // Create the menu action
        let header = Rc::downgrade(&self.headerbar);
        action(&self.window, "menu", move |_, _| {
            header.upgrade().map(|h| h.open_menu());
        });
        // Bind the hamburger menu button to `F10`
        self.instance.set_accels_for_action("win.menu", &["F10"]);
    }

    fn setup_action_channel(&self) -> glib::Continue {
        use crossbeam_channel::TryRecvError;

        let action = match self.receiver.try_recv() {
            Ok(a) => a,
            Err(TryRecvError::Empty) => return glib::Continue(true),
            Err(TryRecvError::Disconnected) => {
                unreachable!("How the hell was the action channel dropped.")
            }
        };

        trace!("Incoming channel action: {:?}", action);
        match action {
            Action::RefreshAllViews => self.content.update(),
            Action::RefreshShowsView => self.content.update_shows_view(),
            Action::RefreshWidgetIfSame(id) => self.content.update_widget_if_same(id),
            Action::RefreshEpisodesView => self.content.update_home(),
            Action::RefreshEpisodesViewBGR => self.content.update_home_if_background(),
            Action::ReplaceWidget(pd) => {
                let shows = self.content.get_shows();
                let pop = shows.borrow().populated();
                pop.borrow_mut()
                    .replace_widget(pd.clone())
                    .map_err(|err| error!("Failed to update ShowWidget: {}", err))
                    .map_err(|_| error!("Failed to update ShowWidget {}", pd.title()))
                    .ok();
            }
            Action::ShowWidgetAnimated => {
                let shows = self.content.get_shows();
                let pop = shows.borrow().populated();
                pop.borrow_mut()
                    .switch_visible(PopulatedState::Widget, gtk::StackTransitionType::SlideLeft);
            }
            Action::ShowShowsAnimated => {
                let shows = self.content.get_shows();
                let pop = shows.borrow().populated();
                pop.borrow_mut()
                    .switch_visible(PopulatedState::View, gtk::StackTransitionType::SlideRight);
            }
            Action::HeaderBarShowTile(title) => self.headerbar.switch_to_back(&title),
            Action::HeaderBarNormal => self.headerbar.switch_to_normal(),
            Action::MarkAllPlayerNotification(pd) => {
                let notif = mark_all_notif(pd, &self.sender);
                notif.show(&self.overlay);
            }
            Action::RemoveShow(pd) => {
                let notif = remove_show_notif(pd, self.sender.clone());
                notif.show(&self.overlay);
            }
            Action::ErrorNotification(err) => {
                error!("An error notification was triggered: {}", err);
                let callback = |revealer: gtk::Revealer| {
                    revealer.set_reveal_child(false);
                    glib::Continue(false)
                };
                let undo_cb: Option<fn()> = None;
                let notif = InAppNotification::new(&err, 6000, callback, undo_cb);
                notif.show(&self.overlay);
            }
            Action::ShowUpdateNotif(receiver) => {
                let sender = self.sender.clone();
                let callback = move |revealer: gtk::Revealer| match receiver.try_recv() {
                    Err(TryRecvError::Empty) => glib::Continue(true),
                    Err(TryRecvError::Disconnected) => glib::Continue(false),
                    Ok(_) => {
                        revealer.set_reveal_child(false);
                        sender
                            .send(Action::RefreshAllViews)
                            .expect("Action channel blew up somehow");
                        glib::Continue(false)
                    }
                };
                let txt = i18n("Fetching new episodes");
                let undo_cb: Option<fn()> = None;
                let updater = InAppNotification::new(&txt, 250, callback, undo_cb);
                updater.set_close_state(State::Hidden);
                updater.set_spinner_state(SpinnerState::Active);

                let old = self.updater.replace(Some(updater));
                old.map(|i| i.destroy());
                self.updater
                    .borrow()
                    .as_ref()
                    .map(|i| i.show(&self.overlay));
            }
            Action::InitEpisode(rowid) => {
                let res = self.player.initialize_episode(rowid);
                debug_assert!(res.is_ok());
            }
            Action::InitShowMenu(s) => {
                let menu = &s.get().container;
                self.headerbar.set_secondary_menu(menu);
            }
            Action::EmptyState => {
                self.window
                    .lookup_action("refresh")
                    .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
                    // Disable refresh action
                    .map(|action| action.set_enabled(false));

                self.headerbar.switch.set_sensitive(false);
                self.content.switch_to_empty_views();
            }
            Action::PopulatedState => {
                self.window
                    .lookup_action("refresh")
                    .and_then(|action| action.downcast::<gio::SimpleAction>().ok())
                    // Enable refresh action
                    .map(|action| action.set_enabled(true));

                self.headerbar.switch.set_sensitive(true);
                self.content.switch_to_populated();
            }
            Action::RaiseWindow => self.window.present(),
        };

        glib::Continue(true)
    }

    pub(crate) fn run() {
        // Set up the textdomain for gettext
        setlocale(LocaleCategory::LcAll, "");
        bindtextdomain("gnome-podcasts", LOCALEDIR);
        textdomain("gnome-podcasts");

        let application = gtk::Application::new(APP_ID, gio::ApplicationFlags::empty())
            .expect("Application initialization failed...");
        application.set_resource_base_path("/org/gnome/Podcasts");

        let weak_app = application.downgrade();
        application.connect_startup(move |_| {
            info!("GApplication::startup");
            weak_app.upgrade().map(|application| {
                let app = Self::new(&application);
                Self::init(&app);

                let weak = Rc::downgrade(&app);
                application.connect_activate(move |_| {
                    info!("GApplication::activate");
                    if let Some(app) = weak.upgrade() {
                        // Ideally Gtk4/GtkBuilder make this irrelvent
                        app.window.show_all();
                        app.window.present();
                        info!("Window presented");
                    } else {
                        debug_assert!(false, "I hate computers");
                    }
                });

                info!("Init complete");
            });
        });

        // Weird magic I copy-pasted that sets the Application Name in the Shell.
        glib::set_application_name(&i18n("Podcasts"));
        glib::set_prgname(Some("gnome-podcasts"));
        gtk::Window::set_default_icon_name(APP_ID);
        let args: Vec<String> = env::args().collect();
        ApplicationExtManual::run(&application, &args);
    }
}
