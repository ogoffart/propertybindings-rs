#![recursion_limit = "4098"]

#[macro_use]
extern crate propertybindings;

#[cfg(target_arch="wasm32")]
extern crate piet_cargoweb as piet_common;

use std::rc::Rc;

pub trait ItemContainer<'a> {
    fn add_child(&self, child: Rc<dyn propertybindings::items::Item<'a> + 'a>);
}

mod wheel {

use piet_common::{Piet, RenderContext};
use propertybindings::items::{Geometry, LayoutInfo, Item, DrawResult, MouseEvent};
use propertybindings::properties::Property;
use std::cell::{RefCell};
use std::rc::Rc;
use super::ItemContainer;

type Timestamp = u64;

thread_local! {
    pub static TIMESTAMP: Property<'static, Timestamp> = Default::default();
}

#[derive(Default, Clone)]
pub struct AnimationInfo {
    pub from: f64,
    pub target: f64,
    pub start_time: Timestamp,
    pub stop_time: Timestamp,
}

/// Can contains other Items, resize the items to the size of the Caintainer
#[derive(Default)]
pub struct WheelLayout<'a> {
    pub geometry: Geometry<'a>,
    pub layout_info: LayoutInfo<'a>,
    children: RefCell<Vec<Rc<dyn Item<'a> + 'a>>>,
    pub angle: Property<'a, f64>,
    pub angle_animation: Property<'a, AnimationInfo>,
    pub item_size: Property<'a, f64>,
}
impl<'a> Item<'a> for WheelLayout<'a> {
    fn geometry(&self) -> &Geometry<'a> {
        &self.geometry
    }
    fn layout_info(&self) -> &LayoutInfo<'a> {
        &self.layout_info
    }

    fn draw(&self, rc: &mut Piet) -> DrawResult {
        let g = self.geometry().to_rect();
        rc.with_save(|rc| {
            rc.transform(piet_common::kurbo::Affine::translate(g.origin().to_vec2()));
            for child in self.children.borrow().iter() {
                child.draw(rc)?
            }
            Ok(())
        })
    }

    fn mouse_event(&self, event: MouseEvent) -> bool {
        for i in self.children.borrow().iter() {
            let g = i.geometry().to_rect();
            if g.contains(event.position()) {
                return i.mouse_event(event.translated(g.origin()));
            }
        }
        return false;
    }
}

impl<'a> ItemContainer<'a> for Rc<WheelLayout<'a>> {
    fn add_child(&self, child: Rc<dyn Item<'a> + 'a>) {
        self.children.borrow_mut().push(child);
        WheelLayout::build_layout(self);
    }
}

impl<'a> WheelLayout<'a> {
    pub fn new() -> Rc<Self> {
        Default::default()
    }

    fn build_layout(this: &Rc<Self>) {
        let count = this.children.borrow().len();
        for (idx, x) in this.children.borrow().iter().enumerate() {
            let w = Rc::downgrade(this);
            x.geometry().width.set_binding(Some(move || Some(w.upgrade()?.item_size.get())));
            let w = Rc::downgrade(this);
            x.geometry().height.set_binding(Some(move || Some(w.upgrade()?.item_size.get())));
            let w = Rc::downgrade(this);
            let a = (idx as f64 * 2. * std::f64::consts::PI) / (count as f64);
            x.geometry().x.set_binding(Some(move || Some({
                let w = w.upgrade()?;
                w.geometry().width() / 2. + w.geometry().width() / 3. * (a + w.angle.get()).cos() - w.item_size.get() / 2.
            })));
            let w = Rc::downgrade(this);
            x.geometry().y.set_binding(Some(move || Some({
                let w = w.upgrade()?;
                w.geometry().height() / 2. + w.geometry().height() / 3. * (a + w.angle.get()).sin() - w.item_size.get() / 2.
            })));
        }
    }

    pub fn set_angle_animated(&self, a: f64) {
        let current = TIMESTAMP.with(|x| x.get());
        let a = AnimationInfo{ from: self.angle.get(), target: a, start_time: current, stop_time: current + 500 };
        self.angle_animation.set(a);
    }
}

}



#[derive(Default)]
struct Wear {
}

impl propertybindings::quick::ItemFactory for Wear {
    fn create() -> Rc<dyn propertybindings::items::Item<'static>> {
        use propertybindings::items::*;
        use wheel::WheelLayout;

//         let button_img = image::load_from_memory(include_bytes!("images/button.png")).ok();

        rsml! { struct Button : Container {
            @signal on_clicked,
//             active: i32,
//             index: i32,
            text: String;
            Image {
                image: image::load_from_memory(include_bytes!("images/button.png")).ok(),
            }
            Text {
                text: Button.text.get(),
                vertical_alignment: alignment::VCENTER,
                horizontal_alignment: alignment::HCENTER,
            }
            MouseArea {
                @id: mouse,
                on_clicked: Button.on_clicked.emit()
            }
        }}

        //let model2 = model.clone();
        let a = -(2. * std::f64::consts::PI) / (8 as f64);

        rsml!(
            Container {
                Image {
                    image: image::load_from_memory(include_bytes!("images/clouds.jpg")).ok(),
                }
                WheelLayout {
                    @id: wheel,
                    item_size: 100.,
                    angle: {
                        let anim = wheel.angle_animation.get();
                        let current = wheel::TIMESTAMP.with(|x|x.get());
                        if current >= anim.stop_time {
                            anim.target
                        } else {
                            let f = (current - anim.start_time) as f64 / (anim.stop_time - anim.start_time) as f64;
                            anim.from * (1. - f) + anim.target * f
                        }
                    },
                    Button { text: "☔".into(), on_clicked: wheel.set_angle_animated(1.*a), }
                    Button { text: "♖".into(), on_clicked: wheel.set_angle_animated(2.*a), }
                    Button { text: "☃".into(), on_clicked: wheel.set_angle_animated(3.*a), }
                    Button { text: "☎".into(), on_clicked: wheel.set_angle_animated(4.*a), }
                    Button { text: "⚙".into(), on_clicked: wheel.set_angle_animated(5.*a), }
                    Button { text: "☀".into(), on_clicked: wheel.set_angle_animated(6.*a), }
                    Button { text: "♿".into(), on_clicked: wheel.set_angle_animated(7.*a), }
                    Button { text: "☪".into(), on_clicked: wheel.set_angle_animated(8.*a), }

                }
            }
        )
    }

    fn tick() {
        wheel::TIMESTAMP.with(|x| x.set(x.get() + 16));
    }
}

fn main() {
    let app = propertybindings::quick::Application::default();
    app.show_window::<Wear>();
}
