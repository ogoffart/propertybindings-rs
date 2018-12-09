use super::items::{Item, MouseEvent};
use qmetaobject::scenegraph::{ContainerNode, SGNode};
use qmetaobject::{QObject, QQuickItem, QRectF};
use std::any::TypeId;
use std::collections::hash_map::DefaultHasher;
use std::ffi::CString;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

/// Use as a factory for RSMLItem
pub trait ItemFactory {
    fn create() -> Rc<Item<'static>>;
}

/// A QQuickItem which is showing an Item
#[derive(QObject)]
pub struct RSMLItem<T: ItemFactory + 'static> {
    base: qt_base_class!(trait QQuickItem),
    node: Option<Rc<Item<'static> + 'static>>,
    _phantom: ::std::marker::PhantomData<T>,
}
impl<T: ItemFactory + 'static> RSMLItem<T> {
    fn set_node(&mut self, node: Rc<Item<'static>>) {
        node.init(self);
        self.node = Some(node);
        {
            //FIXME! I guess qmetaobject::QQuickItem should expose an API for that
            let obj = self.get_cpp_object();
            assert!(!obj.is_null());
            cpp!(unsafe [obj as "QQuickItem*"] {
                obj->setFlag(QQuickItem::ItemHasContents);
                obj->setAcceptedMouseButtons(Qt::LeftButton);
            });
        }
        (self as &QQuickItem).update();
    }
}
impl<T: ItemFactory + 'static> Default for RSMLItem<T> {
    fn default() -> Self {
        RSMLItem {
            base: Default::default(),
            node: None,
            _phantom: Default::default(),
        }
    }
}

impl<T: ItemFactory + 'static> QQuickItem for RSMLItem<T> {
    fn update_paint_node(&mut self, mut node: SGNode<ContainerNode>) -> SGNode<ContainerNode> {
        if let Some(ref i) = self.node {
            node = i.update_paint_node(node, self);
        }
        node
    }

    fn geometry_changed(&mut self, new_geometry: QRectF, _old_geometry: QRectF) {
        if let Some(ref i) = self.node {
            i.geometry().width.set(new_geometry.width);
            i.geometry().height.set(new_geometry.height);
        }
        (self as &QQuickItem).update();
    }

    fn class_begin(&mut self) {
        self.set_node(T::create());
    }

    fn mouse_event(&mut self, event: ::qmetaobject::QMouseEvent) -> bool {
        let pos = event.position();
        let e = match event.event_type() {
            ::qmetaobject::QMouseEventType::MouseButtonPress => MouseEvent::Press(pos),
            ::qmetaobject::QMouseEventType::MouseButtonRelease => MouseEvent::Release(pos),
            ::qmetaobject::QMouseEventType::MouseMove => MouseEvent::Move(pos),
        };
        self.node.as_ref().map_or(false, |n| n.mouse_event(e))
    }
}

/// Show a QQuickWindow showing an instance of the item created by the ItemFactory
pub fn show_window<T: ItemFactory + 'static>() {
    // Compute a somehow unique type name for this type
    let mut hasher = DefaultHasher::new();
    TypeId::of::<T>().hash(&mut hasher);
    let type_name = format!("RSMLItem_{}", hasher.finish());
    let name = CString::new(type_name).unwrap();

    ::qmetaobject::qml_register_type::<RSMLItem<T>>(&name, 1, 0, &name);
    let mut engine = ::qmetaobject::QmlEngine::new();

    engine.load_data(
        format!(
            r#"
import QtQuick 2.0;
import QtQuick.Window 2.0;
import {name} 1.0;
Window {{
    visible: true;
    {name} {{ anchors.fill: parent; }}
}}
        "#,
            name = name.to_str().unwrap()
        ).into(),
    );
    engine.exec();
}
