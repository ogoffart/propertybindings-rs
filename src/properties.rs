use std;
use std::cell::{Cell, RefCell};
use std::convert::From;
use std::default::Default;
use std::ops::DerefMut;
use std::ptr::NonNull;
use std::rc::{Rc, Weak};

// A double linked intrusive list.
// This is unsafe to use.
// It work because the pointer are of type Link for which we know that the data is not moving from
// and that it is droped when it is destroyed.
mod double_link {

    use std::ptr;
    use std::ptr::NonNull;

    pub trait LinkedList {
        type NodeItem;
        unsafe fn next_ptr(node: NonNull<Self::NodeItem>) -> NonNull<Node<Self>>;
    }

    pub struct Node<L: LinkedList + ?Sized> {
        next: *mut L::NodeItem,
        prev: *mut *mut L::NodeItem,
    }

    impl<L: LinkedList + ?Sized> Default for Node<L> {
        fn default() -> Self {
            Node {
                next: ptr::null_mut(),
                prev: ptr::null_mut(),
            }
        }
    }

    impl<L: LinkedList + ?Sized> Drop for Node<L> {
        fn drop(&mut self) {
            if self.prev.is_null() {
                return;
            }
            unsafe {
                if !self.next.is_null() {
                    L::next_ptr(NonNull::new_unchecked(self.next)).as_mut().prev = self.prev;
                }
                *self.prev = self.next;
            }
        }
    }

    struct NodeIter<L: LinkedList + ?Sized>(*mut L::NodeItem);

    impl<L: LinkedList + ?Sized> Iterator for NodeIter<L> {
        type Item = NonNull<L::NodeItem>;
        fn next(&mut self) -> Option<Self::Item> {
            let r = NonNull::new(self.0);
            r.as_ref()
                .map(|n| unsafe { self.0 = L::next_ptr(*n).as_ref().next });
            return r;
        }
    }

    pub struct Head<L: LinkedList + ?Sized>(*mut L::NodeItem);

    impl<L: LinkedList + ?Sized> Default for Head<L> {
        fn default() -> Self {
            Head(ptr::null_mut())
        }
    }

    impl<L: LinkedList + ?Sized> Head<L> {
        pub unsafe fn append(&mut self, node: NonNull<L::NodeItem>) {
            let mut node_node = L::next_ptr(node);
            node_node.as_mut().next = self.0;
            node_node.as_mut().prev = &mut self.0 as *mut *mut L::NodeItem;
            if !self.0.is_null() {
                L::next_ptr(NonNull::new_unchecked(self.0)).as_mut().prev =
                    &mut node_node.as_mut().next as *mut _
            }
            self.0 = node.as_ptr();
        }

        // Not safe because it breaks if the list is modified while iterating.
        fn iter(&mut self) -> NodeIter<L> {
            return NodeIter(self.0);
        }

        pub fn swap(&mut self, other: &mut Self) {
            unsafe {
                ::std::mem::swap(&mut self.0, &mut other.0);
                if !self.0.is_null() {
                    L::next_ptr(NonNull::new_unchecked(self.0)).as_mut().prev = self.0 as *mut _;
                }
                if !other.0.is_null() {
                    L::next_ptr(NonNull::new_unchecked(other.0)).as_mut().prev = other.0 as *mut _;
                }
            }
        }

        pub fn clear(&mut self) {
            unsafe {
                for x in self.iter() {
                    Box::from_raw(x.as_ptr());
                }
            }
        }
    }

    impl<L: LinkedList + ?Sized> Iterator for Head<L> {
        type Item = Box<L::NodeItem>;
        fn next(&mut self) -> Option<Self::Item> {
            NonNull::new(self.0).map(|n| unsafe {
                let mut node_node = L::next_ptr(n);
                self.0 = node_node.as_ref().next;
                if !self.0.is_null() {
                    L::next_ptr(NonNull::new_unchecked(self.0)).as_mut().prev =
                        &mut self.0 as *mut _;
                }
                node_node.as_mut().prev = ::std::ptr::null_mut();
                node_node.as_mut().next = ::std::ptr::null_mut();
                Box::from_raw(n.as_ptr())
            })
        }
    }

    impl<L: LinkedList + ?Sized> Drop for Head<L> {
        fn drop(&mut self) {
            self.clear();
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        enum TestList {}
        #[derive(Default)]
        struct TestNode {
            elem: u32,
            list: Node<TestList>,
        }
        impl LinkedList for TestList {
            type NodeItem = TestNode;
            unsafe fn next_ptr(mut node: NonNull<Self::NodeItem>) -> NonNull<Node<Self>> {
                NonNull::new_unchecked(&mut node.as_mut().list as *mut _)
            }
        }
        impl TestNode {
            fn new(v: u32) -> Self {
                TestNode {
                    elem: v,
                    ..Default::default()
                }
            }
        }

        #[test]
        fn list_append() {
            let mut l: Head<TestList> = Default::default();
            assert_eq!(l.iter().count(), 0);
            unsafe {
                l.append(NonNull::new_unchecked(Box::into_raw(Box::new(
                    TestNode::new(10),
                ))));
            }
            assert_eq!(l.iter().count(), 1);
            unsafe {
                l.append(NonNull::new_unchecked(Box::into_raw(Box::new(
                    TestNode::new(20),
                ))));
            }
            assert_eq!(l.iter().count(), 2);
            unsafe {
                l.append(NonNull::new_unchecked(Box::into_raw(Box::new(
                    TestNode::new(30),
                ))));
            }
            assert_eq!(l.iter().count(), 3);
            assert_eq!(
                l.iter()
                    .map(|x| unsafe { x.as_ref().elem })
                    .collect::<Vec<u32>>(),
                vec![30, 20, 10]
            );
            // take a ptr to the second element;
            let ptr = l.iter().nth(1).unwrap();
            assert_eq!(unsafe { ptr.as_ref().elem }, 20);
            unsafe {
                Box::from_raw(ptr.as_ptr());
            }
            assert_eq!(l.iter().count(), 2);
            assert_eq!(
                l.iter()
                    .map(|x| unsafe { x.as_ref().elem })
                    .collect::<Vec<u32>>(),
                vec![30, 10]
            );
        }
    }

}

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

type WeakPropertyRef = Weak<PropertyBase>;

thread_local!(static CURRENT_PROPERTY: RefCell<Option<WeakPropertyRef>> = Default::default());

fn run_with_current<'a, U, F>(dep: Weak<PropertyBase + 'a>, f: F) -> U
where
    F: Fn() -> U,
{
    let mut old = Some(unsafe {
        // We only leave this for the time we are on this function, so the lifetime is fine
        std::mem::transmute::<Weak<PropertyBase + 'a>, Weak<PropertyBase + 'static>>(dep)
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
    fn update<'a>(&'a self, dep: Weak<PropertyBase + 'a>);
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
    binding: RefCell<Option<Box<PropertyBindingFn<T> + 'a>>>,
    dependencies: RefCell<double_link::Head<NotifyList>>,
    rev_dep: RefCell<double_link::Head<SenderList>>,
    updating: Cell<bool>,
    callbacks: RefCell<Vec<Box<FnMut(&T) + 'a>>>,
}
impl<'a, T> PropertyBase for PropertyImpl<'a, T> {
    fn update<'b>(&'b self, dep: Weak<PropertyBase + 'b>) {
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
    callbacks: RefCell<Vec<Box<PropertyBindingFn<()> + 'a>>>,
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
