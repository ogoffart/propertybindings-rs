#![allow(dead_code)]

use core::cell::{Cell, RefCell};
use core::default::Default;
use core::ptr::NonNull;
use core::pin::Pin;
use std::ops::DerefMut;

// A double linked intrusive list.
// This is unsafe to use.
// It work because the pointer are of type Link for which we know that the data is not moving from
// and that it is droped when it is destroyed.
mod double_link {

    use core::ptr;
    use core::ptr::NonNull;

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
    elem: NonNull<dyn PropertyBase>,
}
impl Link {
    fn new(elem: NonNull<dyn PropertyBase>) -> Self {
        Link {
            notify_list: double_link::Node::default(),
            sender_list: double_link::Node::default(),
            elem,
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


thread_local!(static CURRENT_PROPERTY: RefCell<Option<NonNull<dyn PropertyBase>>> = Default::default());

fn run_with_current<'a, U, F>(dep: NonNull<dyn PropertyBase + 'a>, f: F) -> U
where
    F: Fn() -> U,
{
    let mut old = Some(unsafe {
        // This is safe because we only store it for the duration of the call
        std::mem::transmute::<NonNull<dyn PropertyBase + 'a>, NonNull<dyn PropertyBase + 'static>>(dep)
    });
    CURRENT_PROPERTY.with(|cur_dep| {
        let mut m = cur_dep.borrow_mut();
        std::mem::swap(m.deref_mut(), &mut old);
    });
    let res = f();
    CURRENT_PROPERTY.with(|cur_dep| {
        let mut m = cur_dep.borrow_mut();
        std::mem::swap(m.deref_mut(), &mut old);
        assert_eq!(old, Some(dep));
    });
    res
}

trait PropertyBase {
    fn update(self : Pin<&Self>);
    fn add_dependency(&self, link: NonNull<Link>);
    fn add_rev_dependency(&self, link: NonNull<Link>);
    fn update_dependencies(&self);

    /// For debug purposes only
    fn description(&self) -> String {
        String::default()
    }

    fn accessed(&self) -> bool {
        CURRENT_PROPERTY.with(|cur_dep| {
            if let Some(m) = *cur_dep.borrow() {
                let b = Box::new(Link::new(m));
                let b = unsafe { NonNull::new_unchecked(Box::into_raw(b)) };

                self.add_dependency(b);
                unsafe { m.as_ref().add_rev_dependency(b) };
                return true;
            }
            return false;
        })
    }
}

pub struct Binding<F> {
    rev_dep: Cell<double_link::Head<SenderList>>,
    functor : F,
}

impl<'a, T, F: Fn()->T + 'a> core::convert::From<F> for Binding<F> {
    fn from(f : F) -> Self {
        Binding{ rev_dep: Cell::default(), functor: f }
    }
}

trait BindingBase<'a, T> {
    fn run(&self) -> Option<T>;
    fn add_rev_dependency(&self, link: NonNull<Link>);
    fn clear_dependency(&self);
}

impl<'a, T, F: Fn()->T + 'a> BindingBase<'a, T> for Binding<F> {
    fn run(&self) -> Option<T> {
        return Some((self.functor)())
    }
    fn add_rev_dependency(&self, link: NonNull<Link>) {
        unsafe {
            (&mut *self.rev_dep.as_ptr()).append(link);
        }
    }
    fn clear_dependency(&self) {
        unsafe { &mut *self.rev_dep.as_ptr() }.clear();
    }

}

/// A Property which do not use heap alocation, but is not safe to use because
/// one must ensure that it is not moved
#[derive(Default)]
pub struct PropertyLight<'a, T> {
    value: Cell<T>,
    binding: Cell<Option<&'a dyn BindingBase<'a, T>>>,
    dependencies: Cell<double_link::Head<NotifyList>>,
    // updating: Cell<bool>,
    // callbacks: RefCell<Vec<Box<dyn FnMut(&T) + 'a>>>,
}

impl<'a, T> PropertyBase for PropertyLight<'a, T> {
    fn update(self : Pin<&Self>) {
        if let Some(f) = self.binding.get() {

            /*if self.updating.get() {
                panic!("Circular dependency found : {}", self.description());
            }
            self.updating.set(true);*/
            f.clear_dependency();

            if let Some(val) = run_with_current(NonNull::from(&*self), || f.run()) {
                // FIXME: check that the property does actualy change
                self.value.set(val);
                self.update_dependencies();
            }
            //self.updating.set(false);
        }
    }
    fn add_dependency(&self, link: NonNull<Link>) {
        //println!("ADD DEPENDENCY {} -> {}",  self.description(), dep.upgrade().map_or("NONE".into(), |x| x.description()));
        unsafe {
            (&mut *self.dependencies.as_ptr()).append(link);
        }
    }
    fn add_rev_dependency(&self, link: NonNull<Link>) {
        //println!("ADD REV DEPENDENCY {} -> {}",  self.description(), dep.upgrade().map_or("NONE".into(), |x| x.description()));
        if let Some(f) = self.binding.get() {
            f.add_rev_dependency(link);
        }
    }

    fn update_dependencies(&self) {
        let mut v = Default::default();
        unsafe { &mut *self.dependencies.as_ptr() }.swap(&mut v);
        for d in v {
            let elem = d.elem.clone();
            std::mem::drop(d); // One need to drop it to remove it from the rev list before calling update.
            unsafe { Pin::new_unchecked(elem.as_ref()).update(); }
        }
        /*for cb in self.callbacks.borrow_mut().iter_mut() {
            (*cb)(&self.value.borrow());
        }*/
    }

    /*fn description(&self) -> String {
        if let Some(ref f) = *self.binding.borrow() {
            f.description()
        } else {
            String::default()
        }
    }*/
}

impl<'a, T : Clone> PropertyLight<'a, T> {

    /// Set the value, and notify all the dependent property so their binding can be re-evaluated
    pub fn set(self : Pin<&Self>, t: T) {
        self.binding.set(None);
        self.value.set(t);
        // FIXME! don't update dependency if the property don't change.
        self.update_dependencies();
    }
    pub fn set_binding<F : Fn()->T>(self : Pin<&Self>, f: &'a Binding<F>) {
        self.binding.set(Some(f));
        self.update();
    }

    /// Get the value.
    /// Accessing this property from another's property binding will mark the other property as a dependency.
    pub fn get(self : Pin<&Self>) -> T {
        self.accessed();
        unsafe { &*self.value.as_ptr() }.clone()
    }
}


#[cfg(test)]
mod tests_propertylight {

    use super::*;


    #[derive(Default)]
    struct Rectangle<'a> {
        width: PropertyLight<'a, u32>,
        height: PropertyLight<'a, u32>,
        area: PropertyLight<'a, u32>,
    }

    #[test]
    fn it_works() {
        let r = Rectangle::default();
        //let r = &r2;
        unsafe { Pin::new_unchecked(&r.width) }.set(2);
        let f = Binding::from( || { unsafe { Pin::new_unchecked(&r.width) }.get() * unsafe { Pin::new_unchecked(&r.height) }.get() } );
        unsafe { Pin::new_unchecked(&r.area) }.set_binding( &f );
        unsafe { Pin::new_unchecked(&r.height) }.set(4);
        assert_eq!(unsafe { Pin::new_unchecked(&r.area) }.get(), 4 * 2);
    }


    #[test]
    fn sub_lifetime() {
        let r = Rectangle::default();
        unsafe { Pin::new_unchecked(&r.width) }.set(2);
        {
            let r2 = Rectangle::default();
            let f = Binding::from( || { unsafe { Pin::new_unchecked(&r.width) }.get() * unsafe { Pin::new_unchecked(&r.height) }.get() } );
            unsafe { Pin::new_unchecked(&r2.area) }.set_binding( &f );
            unsafe { Pin::new_unchecked(&r.height) }.set(4);
            assert_eq!(unsafe { Pin::new_unchecked(&r2.area) }.get(), 4 * 2);

            // Must not compile!
            //let f = Binding::from( || { unsafe { Pin::new_unchecked(&r2.width) }.get() * unsafe { Pin::new_unchecked(&r2.height) }.get() } );
            //unsafe { Pin::new_unchecked(&r.area) }.set_binding( &f );
        }
        unsafe { Pin::new_unchecked(&r.height) }.set(42);

        let f = Binding::from(|| {
            let p = PropertyLight::<u32>::default();
            let p = unsafe { Pin::new_unchecked(&p) };
            p.set(21);
            p.get()
        });
        unsafe { Pin::new_unchecked(&r.area) }.set_binding( &f );
        assert_eq!(unsafe { Pin::new_unchecked(&r.area) }.get(), 21);
    }

//     #[test]
//     fn test_notify() {
//         let x = Cell::new(0);
//         let bar = PropertyLight::from(2);
//         let foo = PropertyLight::from(2);
//         foo.on_notify(|_| x.set(x.get() + 1));
//         foo.set(3);
//         assert_eq!(x.get(), 1);
//         foo.set(45);
//         assert_eq!(x.get(), 2);
//         foo.set_binding(|| bar.value());
//         assert_eq!(x.get(), 3);
//         bar.set(8);
//         assert_eq!(x.get(), 4);
//     }
}

/// Property can be assigned bindins which can access other properties. Changing the value of these
/// other properties automatically re-evaluate the bindings
pub struct Property<'a, T> {
    boxed: Pin<Box<PropertyLight<'a, T>>>
}

impl<'a, T : Default> Default for Property<'a, T> {
    fn default() -> Self {
        Property{ boxed: Box::pin(PropertyLight::default()) }
    }
}

impl<'a, T: Clone> Property<'a, T> {
    pub fn set(&self, t: T) {
        self.boxed.as_ref().set(t)
    }
    pub fn set_binding<F : Fn()->T>(&self, f: &'a Binding<F>) {
        self.boxed.as_ref().set_binding(f)
    }

    /// Get the value.
    /// Accessing this property from another's property binding will mark the other property as a dependency.
    pub fn get(&self) -> T {
        self.boxed.as_ref().get()
    }
}


#[cfg(test)]
mod tests_boxedproperty {
    use super::*;

    #[derive(Default)]
    struct Rectangle<'a> {
        width: Property<'a, u32>,
        height: Property<'a, u32>,
        area: Property<'a, u32>,
    }

    #[test]
    fn it_works() {
        let r = Rectangle::default();
        r.width.set(2);
        let f = Binding::from( || { r.width.get() * r.height.get() } );
        r.area.set_binding( &f );
        r.height.set(4);
        assert_eq!(r.area.get(), 4 * 2);
    }


    #[test]
    fn sub_lifetime() {
        let r = Rectangle::default();
        r.width.set(2);
        {
            let r2 = Rectangle::default();
            let f = Binding::from( || { &r.width.get() * &r.height.get() } );
            r2.area.set_binding( &f );
            r.height.set(4);
            assert_eq!(r2.area.get(), 4 * 2);

            // Must not compile!
            //let f = Binding::from( || { r2.width.get() * r2.height.get() } );
            //r.area.set_binding( &f );
        }
        r.height.set(42);

        let f = Binding::from( || {
            let p = Property::<u32>::default();
            let p = &p;
            p.set(21);
            p.get()
        } );
        r.area.set_binding( &f );
        assert_eq!(r.area.get(), 21);
    }
}



/*
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
}*/

