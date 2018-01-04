use gtk;
use gtk::prelude::*;
use diesel::associations::Identifiable;

use hammond_data::dbqueries;
use hammond_data::Podcast;

use utils::get_pixbuf_from_path;
use content::ShowStack;
use app::Action;

use std::sync::mpsc::Sender;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct ShowsPopulated {
    pub container: gtk::Box,
    scrolled_window: gtk::ScrolledWindow,
    flowbox: gtk::FlowBox,
}

impl Default for ShowsPopulated {
    fn default() -> Self {
        let builder = gtk::Builder::new_from_resource("/org/gnome/hammond/gtk/shows_view.ui");
        let container: gtk::Box = builder.get_object("fb_parent").unwrap();
        let scrolled_window: gtk::ScrolledWindow = builder.get_object("scrolled_window").unwrap();
        let flowbox: gtk::FlowBox = builder.get_object("flowbox").unwrap();

        ShowsPopulated {
            container,
            scrolled_window,
            flowbox,
        }
    }
}

impl ShowsPopulated {
    pub fn new(show: Arc<ShowStack>, sender: Sender<Action>) -> ShowsPopulated {
        let pop = ShowsPopulated::default();
        pop.init(show, sender);
        pop
    }

    pub fn init(&self, show: Arc<ShowStack>, sender: Sender<Action>) {
        use gtk::WidgetExt;

        // TODO: handle unwraps.
        self.flowbox
            .connect_child_activated(clone!(show, sender => move |_, child| {
            // This is such an ugly hack...
            let id = WidgetExt::get_name(child).unwrap().parse::<i32>().unwrap();
            let pd = dbqueries::get_podcast_from_id(id).unwrap();

            show.replace_widget(&pd);
            sender.send(Action::HeaderBarShowTile(pd.title().into())).unwrap();
            show.switch_widget_animated();
        }));
        // Populate the flowbox with the Podcasts.
        self.populate_flowbox();
    }

    fn populate_flowbox(&self) {
        let podcasts = dbqueries::get_podcasts();

        if let Ok(pds) = podcasts {
            pds.iter().for_each(|parent| {
                let flowbox_child = ShowsChild::new(parent);
                self.flowbox.add(&flowbox_child.child);
            });
            self.flowbox.show_all();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.flowbox.get_children().is_empty()
    }

    /// Set scrolled window vertical adjustment.
    pub fn set_vadjustment(&self, vadjustment: &gtk::Adjustment) {
        self.scrolled_window.set_vadjustment(vadjustment)
    }
}

#[derive(Debug)]
struct ShowsChild {
    container: gtk::Box,
    cover: gtk::Image,
    child: gtk::FlowBoxChild,
}

impl Default for ShowsChild {
    fn default() -> Self {
        let builder = gtk::Builder::new_from_resource("/org/gnome/hammond/gtk/shows_child.ui");

        let container: gtk::Box = builder.get_object("fb_child").unwrap();
        let cover: gtk::Image = builder.get_object("pd_cover").unwrap();

        let child = gtk::FlowBoxChild::new();
        child.add(&container);

        ShowsChild {
            container,
            cover,
            child,
        }
    }
}

impl ShowsChild {
    pub fn new(pd: &Podcast) -> ShowsChild {
        let child = ShowsChild::default();
        child.init(pd);
        child
    }

    fn init(&self, pd: &Podcast) {
        self.container.set_tooltip_text(pd.title());

        let cover = get_pixbuf_from_path(&pd.clone().into(), 256);
        if let Some(img) = cover {
            self.cover.set_from_pixbuf(&img);
        };

        WidgetExt::set_name(&self.child, &pd.id().to_string());
    }
}
