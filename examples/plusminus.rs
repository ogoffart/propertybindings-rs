//! inspired from the example in https://github.com/antoyo/relm

#![recursion_limit = "4098"]

#[macro_use]
extern crate propertybindings;
extern crate qmetaobject;
use std::rc::Rc;

use propertybindings::properties::Property;

#[derive(Default)]
struct PlusMinus {
    counter: Property<'static, i32>,
}

impl propertybindings::quick::ItemFactory for PlusMinus {
    fn create() -> Rc<propertybindings::items::Item<'static>> {
        use propertybindings::items::*;
        use qmetaobject::{QColor, QString};

        rsml!{ struct Button : Container {
            @signal on_clicked,
            text: QString;
            Rectangle { color: QColor::from_name(if mouse.pressed.get() {"#aaa"} else {"#ccc"} ) }
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

        let model = Rc::new(PlusMinus::default());
        let model1 = model.clone();
        let model2 = model.clone();

        rsml!(
            ColumnLayout {
                Button {
                    text: "-".into(),
                    on_clicked: model1.counter.set(model1.counter.get() + 1)
                }
                Text {
                    text: model.counter.get().to_string().into(),
                    vertical_alignment: alignment::VCENTER,
                    horizontal_alignment: alignment::HCENTER,
                }
                Button {
                    text: "-".into(),
                    on_clicked: model2.counter.set(model2.counter.get() + 1)
                }
            }
        )
    }
}

fn main() {
    propertybindings::quick::show_window::<PlusMinus>();
}
