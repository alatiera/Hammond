use gtk;
use glib;
use gio;
use gtk::prelude::*;
use gio::{ActionMapExt, ApplicationExt, ApplicationExtManual, SimpleActionExt};

use hammond_data::utils::checkup;
use hammond_data::Source;

use headerbar::Header;
use content::Content;
use utils;

use std::rc::Rc;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum Action {
    UpdateSources(Option<Source>),
    RefreshViews,
    RefreshEpisodesViewBGR,
    HeaderBarShowTile(String),
    HeaderBarNormal,
}

#[derive(Debug)]
pub struct App {
    app_instance: gtk::Application,
    window: gtk::Window,
    header: Rc<Header>,
    content: Rc<Content>,
    receiver: Receiver<Action>,
}

impl App {
    pub fn new() -> App {
        let application =
            gtk::Application::new("org.gnome.Hammond", gio::ApplicationFlags::empty())
                .expect("Initialization failed...");

        // Weird magic I copy-pasted that sets the Application Name in the Shell.
        glib::set_application_name("Hammond");
        glib::set_prgname(Some("Hammond"));

        // Create the main window
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_default_size(860, 640);
        window.set_title("Hammond");
        let app_clone = application.clone();
        window.connect_delete_event(move |_, _| {
            app_clone.quit();
            Inhibit(false)
        });

        let (sender, receiver) = channel();

        // Create a content instance
        let content = Content::new(sender.clone());

        // Create the headerbar
        let header = Header::new(content.clone(), sender);

        // Add the Headerbar to the window.
        window.set_titlebar(&header.container);
        // Add the content main stack to the window.
        window.add(&content.get_stack());

        App {
            app_instance: application,
            window,
            header,
            content,
            receiver,
        }
    }

    pub fn setup_actions(&self) {
        // Updates the database and refreshes every view.
        let update = gio::SimpleAction::new("update", None);
        let content = self.content.clone();
        let header = self.header.clone();
        update.connect_activate(move |_, _| {
            utils::refresh_feed(content.clone(), header.clone(), None);
        });
        self.app_instance.add_action(&update);
    }

    pub fn setup_timed_callbacks(&self) {
        let content = self.content.clone();
        let header = self.header.clone();
        // Update the feeds right after the Application is initialized.
        gtk::timeout_add_seconds(
            2,
            clone!(content => move || {
            utils::refresh_feed(content.clone(), header.clone(), None);
            glib::Continue(false)
        }),
        );

        let content = self.content.clone();
        let header = self.header.clone();
        // Auto-updater, runs every hour.
        // TODO: expose the interval in which it run to a user setting.
        // TODO: show notifications.
        gtk::timeout_add_seconds(
            3600,
            clone!(content => move || {
            utils::refresh_feed(content.clone(), header.clone(), None);
            glib::Continue(true)
        }),
        );

        // Run a database checkup once the application is initialized.
        gtk::timeout_add(300, || {
            let _ = checkup();
            glib::Continue(false)
        });
    }

    pub fn run(self) {
        let window = self.window.clone();
        let app = self.app_instance.clone();
        self.app_instance.connect_startup(move |_| {
            build_ui(&window, &app);
        });
        self.setup_timed_callbacks();
        self.setup_actions();

        let receiver = self.receiver;
        let content = self.content.clone();
        let headerbar = self.header.clone();
        gtk::idle_add(clone!(content, headerbar => move || {
            match receiver.recv_timeout(Duration::from_millis(5)) {
                Ok(Action::UpdateSources(source)) => {
                    if let Some(s) = source {
                        utils::refresh_feed(content.clone(), headerbar.clone(), Some(vec!(s)))
                    }
                }
                Ok(Action::RefreshViews) => {
                    content.update();
                }
                Ok(Action::RefreshEpisodesViewBGR) => {
                    content.update_episode_view_if_baground();
                }
                Ok(Action::HeaderBarShowTile(title)) => {
                    headerbar.switch_to_back(&title)
                }
                Ok(Action::HeaderBarNormal) => {
                    headerbar.switch_to_normal()
                }
                _ => (),
            }

            Continue(true)
        }));

        ApplicationExtManual::run(&self.app_instance, &[]);
    }
}

fn build_ui(window: &gtk::Window, app: &gtk::Application) {
    window.set_application(app);
    window.show_all();
    window.activate();
    app.connect_activate(move |_| ());
}
