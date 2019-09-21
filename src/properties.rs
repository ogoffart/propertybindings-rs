use std;
use std::cell::{Cell, RefCell};
use std::convert::From;
use std::default::Default;
use std::ops::DerefMut;
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

#[path="double_link.rs"]
mod double_link;

enum NotifyList {}
enum SenderList {}

struct Link {
    notify_list: double_link::Node<NotifyList>,
    sender_list: double_link::Node<SenderList>,
    elem: WeakPropertyRef,
}
impl Link {
    fn new(elem: WeakPropertyRef) -> Self {
        Link {
            notify_list: double_link::Node::default(),
            sender_list: double_link::Node::default(),
            elem: elem,
        }
    }
}

impl double_link::LinkedList for NotifyList {
    type NodeItem = Link;
    unsafe fn next_ptr(mut node: NonNull<Self::NodeItem>) -> NonNull<double_link::Node<Self>> {
        NonNull::new_unchecked(&mut node.as_mut().notify_list as *mut _)
    }
}

impl double_link::LinkedList for SenderList {
    type NodeItem = Link;
    unsafe fn next_ptr(mut node: NonNull<Self::NodeItem>) -> NonNull<double_link::Node<Self>> {
        NonNull::new_unchecked(&mut node.as_mut().sender_list as *mut _)
    }
}

type WeakPropertyRef = Weak<dyn PropertyBase>;

thread_local!(static CURRENT_PROPERTY: RefCell<Option<WeakPropertyRef>> = Default::default());

fn run_with_current<'a, U, F>(dep: Weak<dyn PropertyBase + 'a>, f: F) -> U
where
    F: Fn() -> U,
{
    let mut old = Some(unsafe {
        // We only leave this for the time we are on this function, so the lifetime is fine
        std::mem::transmute::<Weak<dyn PropertyBase + 'a>, Weak<dyn PropertyBase + 'static>>(dep)
    });
    CURRENT_PROPERTY.with(|cur_dep| {
        let mut m = cur_dep.borrow_mut();
        std::mem::swap(m.deref_mut(), &mut old);
    });
    let res = f();
    CURRENT_PROPERTY.with(|cur_dep| {
        let mut m = cur_dep.borrow_mut();
        std::mem::swap(m.deref_mut(), &mut old);
        //assert!(Rc::ptr_eq(&dep.upgrade().unwrap(), &old.unwrap().upgrade().unwrap()));
    });
    res
}

trait PropertyBase {
    fn update<'a>(&'a self, dep: Weak<dyn PropertyBase + 'a>);
    fn add_dependency(&self, link: NonNull<Link>);
    fn add_rev_dependency(&self, link: NonNull<Link>);
    fn update_dependencies(&self);

    fn description(&self) -> String {
        String::default()
    }

    fn accessed(&self) -> bool {
        CURRENT_PROPERTY.with(|cur_dep| {
            if let Some(m) = (*cur_dep.borrow()).clone() {
                if let Some(mu) = m.upgrade() {
                    let b = Box::new(Link::new(m));
                    let b = unsafe { NonNull::new_unchecked(Box::into_raw(b)) };

                    self.add_dependency(b);
                    mu.add_rev_dependency(b);
                    return true;
                }
            }
            return false;
        })
    }
}

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

#[derive(Default)]
struct PropertyImpl<'a, T> {
    value: RefCell<T>,
    binding: RefCell<Option<Box<dyn PropertyBindingFn<T> + 'a>>>,
    dependencies: RefCell<double_link::Head<NotifyList>>,
    rev_dep: RefCell<double_link::Head<SenderList>>,
    updating: Cell<bool>,
    callbacks: RefCell<Vec<Box<dyn FnMut(&T) + 'a>>>,
}
impl<'a, T> PropertyBase for PropertyImpl<'a, T> {
    fn update<'b>(&'b self, dep: Weak<dyn PropertyBase + 'b>) {
        if let Some(ref f) = *self.binding.borrow() {
            if self.updating.get() {
                panic!("Circular dependency found : {}", self.description());
            }
            self.updating.set(true);
            self.rev_dep.borrow_mut().clear();

            if let Some(val) = run_with_current(dep, || f.run()) {
                // FIXME: check that the property does actualy change
                *self.value.borrow_mut() = val;
                self.update_dependencies();
            }
            self.updating.set(false);
        }
    }
    fn add_dependency(&self, link: NonNull<Link>) {
        //println!("ADD DEPENDENCY {} -> {}",  self.description(), dep.upgrade().map_or("NONE".into(), |x| x.description()));
        unsafe {
            self.dependencies.borrow_mut().append(link);
        }
    }
    fn add_rev_dependency(&self, link: NonNull<Link>) {
        //println!("ADD DEPENDENCY {} -> {}",  self.description(), dep.upgrade().map_or("NONE".into(), |x| x.description()));
        unsafe {
            self.rev_dep.borrow_mut().append(link);
        }
    }

    fn update_dependencies(&self) {
        let mut v = Default::default();
        {
            let mut dep = self.dependencies.borrow_mut();
            dep.deref_mut().swap(&mut v);
        }
        for d in v {
            let elem = d.elem.clone();
            std::mem::drop(d); // One need to drop it to remove it from the rev list before calling update.
            if let Some(d) = elem.upgrade() {
                let w = Rc::downgrade(&d);
                d.update(w);
            }
        }
        for cb in self.callbacks.borrow_mut().iter_mut() {
            (*cb)(&self.value.borrow());
        }
    }

    fn description(&self) -> String {
        if let Some(ref f) = *self.binding.borrow() {
            f.description()
        } else {
            String::default()
        }
    }
}

#[derive(Default, Clone)]
pub struct WeakProperty<'a, T> {
    d: Weak<PropertyImpl<'a, T>>,
}
impl<'a, T: Default + Clone> WeakProperty<'a, T> {
    pub fn get(&self) -> Option<T> {
        self.d.upgrade().map(|x| (Property { d: x }).get())
    }
}

/// A Property represents a value which records when it is accessed. If the property's binding
/// depends on others property, the property binding is automatically re-evaluated.
// Fixme! the property should maybe be computed lazily, or the graph studied to avoid unnecesseray re-computation.
#[derive(Default)]
pub struct Property<'a, T> {
    d: Rc<PropertyImpl<'a, T>>,
}
impl<'a, T: Default + Clone> Property<'a, T> {
    pub fn from_binding<F: PropertyBindingFn<T> + 'a>(f: F) -> Property<'a, T> {
        let d = Rc::new(PropertyImpl {
            binding: RefCell::new(Some(Box::new(f))),
            ..Default::default()
        });
        let w = Rc::downgrade(&d);
        d.update(w);
        Property { d: d }
    }

    /// Set the value, and notify all the dependent property so their binding can be re-evaluated
    pub fn set(&self, t: T) {
        *self.d.binding.borrow_mut() = None;
        *self.d.value.borrow_mut() = t;
        // FIXME! don't updae dependency if the property don't change.
        self.d.update_dependencies();
    }
    pub fn set_binding<F: PropertyBindingFn<T> + 'a>(&self, f: F) {
        *self.d.binding.borrow_mut() = Some(Box::new(f));
        let w = Rc::downgrade(&self.d);
        self.d.update(w);
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
        self.d.accessed();
        self.d.value.borrow().clone()
    }

    pub fn as_weak(&self) -> WeakProperty<'a, T> {
        WeakProperty {
            d: Rc::downgrade(&self.d),
        }
    }

    /// One can add callback which are being called when the property changes.
    pub fn on_notify<F>(&self, callback: F)
    where
        F: FnMut(&T) + 'a,
    {
        self.d.callbacks.borrow_mut().push(Box::new(callback));
    }
}
impl<'a, T: Default> From<T> for Property<'a, T> {
    fn from(t: T) -> Self {
        Property {
            d: Rc::new(PropertyImpl {
                value: RefCell::new(t),
                ..Default::default()
            }),
        }
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
