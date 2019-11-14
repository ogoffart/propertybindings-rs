//! This crate is work in progress.
//! The Idea is to have develop a QML-inspired macros in rust.
//!
//! Behind the scene, this uses the QML scene graph. But there is
//! only one QQuickItem. All rust Item are just node in the scene
//! graphs.
//! (For some node such as text node, there is an hidden QQuickItem
//! because there is no public API to get a text node)
//! only the `items` module depends on Qt.

#![recursion_limit = "512"]

#[cfg(target_arch="wasm32")]
extern crate piet_cargoweb as piet_common;


#[macro_use]
pub mod properties;
pub use crate::properties::*;
// pub mod anchors;
#[macro_use]
pub mod rslm;
pub mod items;
pub mod quick;

pub mod properties_impl;

mod pin_weak {
    use core::pin::Pin;
    use std::rc::{Rc, Weak};


    /// Like a std::rc::Weak, but can be constructed from a Pin<Rc>
    pub struct PinWeak<T>(Weak<T>);
    impl<T> Default for PinWeak<T> {
        fn default() -> Self { PinWeak(Default::default()) }
    }
    impl<T> Clone for PinWeak<T> {
        fn clone(&self) -> Self { PinWeak(self.0.clone()) }
    }
    impl<T> PinWeak<T> {
        //pub fn new() -> Self { Default::default() }

        pub fn upgrade(&self) -> Option<Pin<Rc<T>>> {
            self.0.upgrade().map(|r| unsafe {
                // This is safe because only created from a Pin<Rc<T>
                Pin::new_unchecked(r)
            })
        }

        pub fn downgrade_from(p: &Pin<Rc<T>>) -> Self {
            Self(Rc::downgrade(unsafe {
                // FIXME: use Pin::into_inner_unchecked
                std::mem::transmute::<&Pin<Rc<T>>, &Rc<T>, >(&p)
            }))
        }
    }

}
