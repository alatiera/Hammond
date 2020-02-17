// base_view.rs
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

use crate::utils::smooth_scroll_to;
use gtk::{self, prelude::*, Adjustment, Orientation, PolicyType};

#[derive(Debug, Clone)]
pub(crate) struct BaseView {
    container: gtk::Box,
    scrolled_window: gtk::ScrolledWindow,
}

impl Default for BaseView {
    fn default() -> Self {
        let container = gtk::Box::new(Orientation::Horizontal, 0);
        // TODO: Remember to file an issue about this API
        // error[E0283]: type annotations required: cannot resolve `_: gdk_pixbuf::IsA<gtk::Adjustment>`
        // --> ../podcasts-gtk/src/widgets/base_view.rs:32:31
        //    |
        // 32 |         let scrolled_window = gtk::ScrolledWindow::new(None, None);
        //    |                               ^^^^^^^^^^^^^^^^^^^^^^^^
        //    |
        //    = note: required by `gtk::ScrolledWindow::new`
        //
        // error: aborting due to previous error
        let foo: Option<&Adjustment> = None;
        let scrolled_window = gtk::ScrolledWindow::new(foo.clone(), foo.clone());

        scrolled_window.set_policy(PolicyType::Never, PolicyType::Automatic);
        container.set_size_request(360, -1);
        container.add(&scrolled_window);
        container.show_all();

        BaseView {
            container,
            scrolled_window,
        }
    }
}

impl BaseView {
    pub(crate) fn container(&self) -> &gtk::Box {
        &self.container
    }

    pub(crate) fn scrolled_window(&self) -> &gtk::ScrolledWindow {
        &self.scrolled_window
    }

    pub(crate) fn add<T: IsA<gtk::Widget>>(&self, widget: &T) {
        self.scrolled_window.add(widget);
    }

    pub(crate) fn set_adjustments<'a, 'b>(
        &self,
        hadjustment: Option<&'a Adjustment>,
        vadjustment: Option<&'b Adjustment>,
    ) {
        if let Some(h) = hadjustment {
            smooth_scroll_to(&self.scrolled_window, h);
        }

        if let Some(v) = vadjustment {
            smooth_scroll_to(&self.scrolled_window, v);
        }
    }

    pub(crate) fn get_vadjustment(&self) -> Option<Adjustment> {
        self.scrolled_window().get_vadjustment()
    }
}
