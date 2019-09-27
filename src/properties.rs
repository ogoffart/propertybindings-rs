//! This module is the old implementation of the property system, before Pin existed.
//! The module properties_impl contains the new implementation and is used as implementation for this
use crate::properties_impl;
use std;
use std::cell::RefCell;
use std::convert::From;
use std::default::Default;
use std::marker::PhantomData;
use std::pin::Pin;
use std::rc::{Rc, Weak};

/// A binding is a function that returns a value of type T
pub trait PropertyBindingFn<T> {
    fn run(&self) -> Option<T>;
    fn description(&self) -> String {
        String::default()
    }
}
impl<F, T> PropertyBindingFn<T> for F
where
    F: Fn() -> T,
{
    fn run(&self) -> Option<T> {
        Some((*self)())
    }
}
// Ideallly this should just be
// impl<F, T> PropertyBindingFn<T> for F where F : Fn()->Option<T>
// But that'd be ambiguous,  so wrap it in an option, even if it is ridiculous.
// Fixme: is there a better solution
impl<F, T> PropertyBindingFn<T> for Option<F>
where
    F: Fn() -> Option<T>,
{
    fn run(&self) -> Option<T> {
        self.as_ref().and_then(|x| x())
    }
}
// This one is usefull for debugging.
impl<F, T> PropertyBindingFn<T> for (String, F)
where
    F: Fn() -> Option<T>,
{
    fn run(&self) -> Option<T> {
        (self.1)()
    }
    fn description(&self) -> String {
        (self.0).clone()
    }
}

#[derive(Default, Clone)]
pub struct WeakProperty<'a, T> {
    d: Weak<properties_impl::Property<T>>,
    phantom: PhantomData<&'a ()>,
}
impl<'a, T: Default + Clone> WeakProperty<'a, T> {
    pub fn get(&self) -> Option<T> {
        // Safe because the original RC is pinned
        self.d
            .upgrade()
            .map(|x| unsafe { Pin::new_unchecked(x) }.as_ref().get())
    }
}

/// A Property represents a value which records when it is accessed. If the property's binding
/// depends on others property, the property binding is automatically re-evaluated.
// Fixme! the property should maybe be computed lazily, or the graph studied to avoid unnecesseray re-computation.
pub struct Property<'a, T: Default> {
    d: Pin<Rc<properties_impl::Property<T>>>,
    callbacks: RefCell<Vec<Pin<Box<properties_impl::ChangeEvent<dyn Fn() + 'a>>>>>,
}
impl<'a, T: Default> Default for Property<'a, T> {
    fn default() -> Self {
        Property {
            d: Rc::pin(properties_impl::Property::default()),
            callbacks: Default::default(),
        }
    }
}
impl<'a, T: Default + Clone> Property<'a, T> {
    pub fn from_binding<F: PropertyBindingFn<T> + 'a>(f: F) -> Property<'a, T> {
        let d = Rc::pin(properties_impl::Property::default());
        d.as_ref().set_binding_owned(move || f.run().unwrap());
        Property {
            d,
            callbacks: Default::default(),
        }
    }

    /// Set the value, and notify all the dependent property so their binding can be re-evaluated
    pub fn set(&self, t: T) {
        self.d.as_ref().set(t);
    }
    pub fn set_binding<F: PropertyBindingFn<T> + 'a>(&self, f: F) {
        self.d.as_ref().set_binding_owned(move || f.run().unwrap());
    }

    /*
    pub fn borrow<'b>(&'b self) -> Ref<'b, T> {
        self.d.accessed();
        let d = self.d.borrow();
        Ref::map(d, |d| &d.value)
    }*/

    // FIXME! remove
    pub fn value(&self) -> T {
        self.get()
    }

    /// Get the value.
    /// Accessing this property from another's property binding will mark the other property as a dependency.
    pub fn get(&self) -> T {
        self.d.as_ref().get()
    }

    pub fn as_weak(&self) -> WeakProperty<'a, T> {
        WeakProperty {
            d: Rc::downgrade(unsafe {
                // FIXME: use Pin::into_inner_unchecked
                std::mem::transmute::<
                    &std::pin::Pin<std::rc::Rc<properties_impl::Property<T>>>,
                    &std::rc::Rc<properties_impl::Property<T>>,
                >(&self.d)
            }),
            phantom: PhantomData,
        }
    }

    /// One can add callback which are being called when the property changes.
    pub fn on_notify<F>(&self, callback: F)
    where
        F: Fn(&T) + 'a,
        T: 'a,
    {
        let d = self.d.clone();
        let e = Box::pin(properties_impl::ChangeEvent::new(move || {
            callback(&d.as_ref().get())
        }));
        e.as_ref().listen(self.d.as_ref());
        self.callbacks.borrow_mut().push(e);
    }
}
impl<'a, T: Default + Clone> From<T> for Property<'a, T> {
    fn from(t: T) -> Self {
        let p = Property::default();
        p.set(t);
        p
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[derive(Default)]
    struct Rectangle<'a> {
        /*
        property<rectangle*> parent = nullptr;
        property<int> width = 150;
        property<int> height = 75;
        property<int> area = [&]{ return calculateArea(width, height); };

        property<std::string> color = [&]{
            if (parent() && area > parent()->area)
            return std::string("blue");
            else
            return std::string("red");
        };*/
        width: Property<'a, u32>,
        height: Property<'a, u32>,
        area: Property<'a, u32>,
    }

    /*
    impl<'a> Rectangle<'a> {
        fn new()->Self {
            Rectangle  { ..Default::default() }
        }
    }*/

    #[test]
    fn it_works() {
        let rec = Rc::new(RefCell::new(Rectangle::default()));
        rec.borrow_mut().width = Property::from(2);
        let wr = Rc::downgrade(&rec);
        rec.borrow_mut().area = Property::from_binding(move || {
            wr.upgrade()
                .map(|wr| wr.borrow().width.value() * wr.borrow().height.value())
                .unwrap()
        });
        rec.borrow().height.set(4);
        assert_eq!(rec.borrow().area.value(), 4 * 2);
    }

    #[test]
    fn test_notify() {
        let x = Cell::new(0);
        let bar = Property::from(2);
        let foo = Property::from(2);
        foo.on_notify(|_| x.set(x.get() + 1));
        foo.set(3);
        assert_eq!(x.get(), 1);
        foo.set(45);
        assert_eq!(x.get(), 2);
        foo.set_binding(|| bar.value());
        assert_eq!(x.get(), 3);
        bar.set(8);
        assert_eq!(x.get(), 4);
    }
}

/// A Signal.
#[derive(Default)]
pub struct Signal<'a> {
    callbacks: RefCell<Vec<Box<dyn PropertyBindingFn<()> + 'a>>>,
}

impl<'a> Signal<'a> {
    pub fn set_binding<F: PropertyBindingFn<()> + 'a>(&self, f: F) {
        self.callbacks.borrow_mut().push(Box::new(f));
    }

    pub fn emit(&self) {
        for cb in self.callbacks.borrow_mut().iter_mut() {
            cb.run();
        }
    }
}
