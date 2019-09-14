#![allow(dead_code)]

use core::cell::{Cell, RefCell};
use core::default::Default;
use core::ptr::NonNull;
use core::pin::Pin;
use vptr::ThinRef;
use std::ops::DerefMut;

mod internal {
    /// Internal struct used by the macro generated code
    /// Copy of core::raw::TraitObject since it is unstable
    #[doc(hidden)]
    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct TraitObject {
        pub data: *const (),
        pub vtable: *const (),
    }
}


#[path="double_link.rs"]
mod double_link;

enum NotifyList {}
enum SenderList {}

struct DependencyNode {
    notify_list: double_link::Node<NotifyList>,
    sender_list: double_link::Node<SenderList>,
    elem: NonNull<dyn NotificationReciever>,
}
impl DependencyNode {
    fn new(elem: NonNull<dyn NotificationReciever>) -> Self {
        DependencyNode {
            notify_list: double_link::Node::default(),
            sender_list: double_link::Node::default(),
            elem,
        }
    }
}

impl double_link::LinkedList for NotifyList {
    type NodeItem = DependencyNode;
    unsafe fn next_ptr(mut node: NonNull<Self::NodeItem>) -> NonNull<double_link::Node<Self>> {
        NonNull::new_unchecked(&mut node.as_mut().notify_list as *mut _)
    }
}

impl double_link::LinkedList for SenderList {
    type NodeItem = DependencyNode;
    unsafe fn next_ptr(mut node: NonNull<Self::NodeItem>) -> NonNull<double_link::Node<Self>> {
        NonNull::new_unchecked(&mut node.as_mut().sender_list as *mut _)
    }
}

thread_local!(static CURRENT_PROPERTY: RefCell<Option<Pin<&'static dyn NotificationReciever>>>
    = Default::default());

fn run_with_current<U, F>(dep: Pin<&dyn NotificationReciever>, f: F) -> U
where
    F: Fn() -> U,
{
    let mut old = Some(unsafe {
        // This is safe because we only store it for the duration of the call
        std::mem::transmute::<Pin<&dyn NotificationReciever>,
            Pin<&'static dyn NotificationReciever>>(dep)
    });
    CURRENT_PROPERTY.with(|cur_dep| {
        let mut m = cur_dep.borrow_mut();
        std::mem::swap(m.deref_mut(), &mut old);
    });
    let res = f();
    CURRENT_PROPERTY.with(|cur_dep| {
        let mut m = cur_dep.borrow_mut();
        std::mem::swap(m.deref_mut(), &mut old);
        //assert_eq!(old, Some(dep));
    });
    res
}

trait NotificationReciever {
    fn notify(self : Pin<&Self>);
    fn add_rev_dependency(self: Pin<&Self>, link: NonNull<DependencyNode>);
}

trait PropertyBase {
    fn add_dependency(&self, link: NonNull<DependencyNode>);
    fn update_dependencies(&self);

    /// For debug purposes only
    fn description(&self) -> String {
        String::default()
    }

    fn accessed(&self) -> bool {
        CURRENT_PROPERTY.with(|cur_dep| {
            if let Some(m) = *cur_dep.borrow() {
                let b = Box::new(DependencyNode::new((&*m).into()));
                let b = unsafe { NonNull::new_unchecked(Box::into_raw(b)) };

                self.add_dependency(b);
                m.as_ref().add_rev_dependency(b);
                return true;
            }
            return false;
        })
    }

}

pub trait Binding<T> {
    fn storage<'a>(self : Pin<&'a Self>) -> Pin<&'a BindingStorage>;
    fn call(self: Pin<&Self>) -> T;
}

#[derive(Default)]
pub struct BindingStorage {
    /// link to the list of properties upon which we depends
    rev_dep: Cell<double_link::Head<SenderList>>,
    /// link to the list of properties that depends on us
    // TODO: have static node, also no need for double link
    notify_dep: Cell<double_link::Head<NotifyList>>,
}

#[derive(Default)]
struct PropertyInternal<T> {
    // if value & 1 { ThinRef<&dyn Binding<T>> } else { double_link::Head<NotifyList> }
    value: core::cell::Cell<usize>,
    phantom: core::marker::PhantomData<(T, std::marker::PhantomPinned)>
}

impl<T> PropertyInternal<T> {
    unsafe fn binding<'a>(&'a self) -> Option<Pin<&'a dyn Binding<T>>> {
        let v = self.value.get();
        if v & 1 == 1 {
            let v = v & (!1);
            Some(std::mem::transmute::<&usize, &'a Pin<ThinRef<'a, dyn Binding<T>>>>(&v).as_ref())
        } else {
            None
        }
    }

    unsafe fn notify_dep<'a>(&'a self) -> &'a Cell<double_link::Head<NotifyList>> {
        self.binding().map(|b| &b.storage().get_ref().notify_dep).unwrap_or_else(||
            std::mem::transmute::<_, &'a Cell<double_link::Head<NotifyList>>>(&self.value))
    }

    unsafe fn set_binding<'a>(&'a self, b: Pin<ThinRef<'a, dyn Binding<T> + 'a>>) {
        let v = std::mem::transmute::<Pin<ThinRef<'a, dyn Binding<T> + 'a>>, usize>(b) | 1usize;
        if self.value.get() == v { return };
        (*self.notify_dep().as_ptr()).swap(&mut *b.as_ref().storage().notify_dep.as_ptr());
        // FIXME! add an is_empty to the list
        //assert_eq!(std::mem::transmute::<_, usize>(*self.notify_dep()), 0);
        self.value.set(v);
    }

    unsafe fn remove_binding(&self) {
        if let Some(b) = self.binding() {
            self.value.set(0);
            (*self.notify_dep().as_ptr()).swap(&mut *b.as_ref().storage().notify_dep.as_ptr());
        }
    }
}

#[derive(Default)]
#[repr(C)]
pub struct Property<T> {
    internal: PropertyInternal<T>,
    value: core::cell::UnsafeCell<T>,
}

impl<T : Clone> Property<T> {
    pub fn set(self : Pin<&Self>, t: T) {
        unsafe { self.internal.remove_binding() };
        unsafe { *self.value.get() = t }
        self.update_dependencies();
    }
    pub fn set_binding<'a>(self : Pin<&'a Self>, b: Pin<ThinRef<'a, dyn Binding<T> + 'a>>) {
        unsafe { self.internal.set_binding(b) };
        self.notify();
    }

    pub fn get(self : Pin<&Self>) -> T {
        self.accessed();
        unsafe { &*self.value.get() }.clone()
    }
}

impl<T> NotificationReciever for Property<T> {
    fn notify(self : Pin<&Self>) {
        if let Some(b) = unsafe { self.internal.binding() } {

            /*if self.updating.get() {
                panic!("Circular dependency found : {}", self.description());
            }
            self.updating.set(true);*/
            // clear dependency
            unsafe { &mut *b.storage().rev_dep.as_ptr() }.clear();

            let val = run_with_current(self, || b.call());
            // FIXME: check that the property does actualy change
            unsafe { *self.value.get() = val }
            self.update_dependencies();
            //self.updating.set(false);
        }
    }
    fn add_rev_dependency(self : Pin<&Self>, link: NonNull<DependencyNode>) {
        unsafe {
            self.internal.binding().map(|b| (&mut *b.storage().rev_dep.as_ptr()).append(link) );
        }
    }

}

impl<T> PropertyBase for Property<T> {
    fn add_dependency(&self, link: NonNull<DependencyNode>) {
        unsafe {
            (&mut *self.internal.notify_dep().as_ptr()).append(link);
        }
    }
    fn update_dependencies(&self) {
        let mut v = Default::default();
        unsafe { &mut *self.internal.notify_dep().as_ptr() }.swap(&mut v);
        for d in v {
            let elem = d.elem.clone();
            std::mem::drop(d); // One need to drop it to remove it from the rev list before calling update.
            unsafe { Pin::new_unchecked(elem.as_ref()).notify(); }
        }
    }
}


#[cfg(test)]
mod t {

    use super::*;

    macro_rules! unsafe_pinned {
        ($v:vis $f:ident: $t:ty) => (
            $v fn $f<'__a>(
                self: ::core::pin::Pin<&'__a  Self>
            ) -> ::core::pin::Pin<&'__a  $t> {
                unsafe {
                    ::core::pin::Pin::map_unchecked(
                        self, |x| & x.$f
                    )
                }
            }
        )
    }

    #[test]
    fn test_property() {
/*
        struct AreaBinding(BindingStorage)
        impl AreaBinding {
            fn call(i : Pin<&Item>) -> f32 {
                i.width().get() * i.height().get()
            }
        }*/


        #[vptr::vptr("Binding<f32>")]
        #[derive(Default)]
        struct AreaBinding<'a>(Option<Pin<&'a Item>>, BindingStorage);
        impl<'a> Binding<f32> for AreaBinding<'a> {
            fn storage(self : Pin<&Self>) -> Pin<&BindingStorage> {
                unsafe {  ::core::pin::Pin::map_unchecked(self, |x| & x.1) }
            }
            fn call(self: Pin<&Self>) -> f32 {
                self.0.map(|i| {i.height().get() * i.width().get()}).unwrap_or(-1.)
            }
        }

        #[derive(Default)]
        struct Item {
            pub width: Property<f32>,
            pub height: Property<f32>,
            pub area: Property<f32>,
            //area1_binding: AreaBinding,
        }

        impl Item {
            unsafe_pinned!(pub width: Property<f32>);
            unsafe_pinned!(pub height: Property<f32>);
            unsafe_pinned!(pub area: Property<f32>);
            //unsafe_pinned!(area1_binding: AreaBinding);


           /* fn init(self: Pin<&mut Self>) /*-> Pin<&Self> */{
                self.area1_binding.1 = Some()
                self.as_ref().height().set(42.);
                self.as_ref().width().set(77.);
//                let mut s = self;
//                 init_binding!(area1_binding <f32> in s:Item {
//                     s.width().get() * s.height().get()
//                 });
//                s.into_ref()
            }*/
        }

        use vptr::prelude::*;

        let i = Item::default();
        pin_utils::pin_mut!(i);
        let i = i.as_ref();
        let mut area_binding = AreaBinding::default();
        area_binding.0 = Some(i);
        pin_utils::pin_mut!(area_binding);
        i.height().set(12.);
        i.width().set(8.);
        i.area().set_binding(area_binding.as_ref().as_pin_thin_ref());
        assert_eq!(i.area().get(), 12.*8.);
        i.width().set(4.);
        assert_eq!(i.area().get(), 12.*4.);

    }
}

/*
trait Binding<T> {}

trait Property<T> {
   /* fn get(self : Pin<&Self>) -> T;
    fn set(self : Pin<&Self>, t : T);
    fn set_binding(self: Pin<&Self>, binding: Pin<&dyn Binding<T>>);*/
}

trait Item {
    type width: Property<f32>;
    type height: Property<f32>;
    type area: Property<f32>;
}



/*
trait Rectangle : Item {
    type color: Property<f32>;
}

trait Button : Rectangle {}

fn Item() -> impl Item {
    struct A;
    impl Property<f32> for A {};
    struct S;
    impl Item for () {
        type width = A;
        type height = A;
        type area = A;
    }

}
*/

mod xxx {
/*
trait Item {
    fn width(&self) -> Property<f32>;
    fn height(&self) -> Property<f32>;

};


fn Item() -> impl Item

*/
}



/*
struct Item<S> {
    pub width: Property<S, f32>,
    pub height: Property<S, f32>,
    pub area1: Property<S, f32>,
}

impl<S> Item<S> {
    unsafe_pinned!(width: Property<S, f32>);
    unsafe_pinned!(height: Property<S, f32>);
    unsafe_pinned!(area1: Property<S, f32>);
}



struct Rectangle<S> {
    pub base: Item<S, f32>,
    pub color: Property<S, f32>,
    pub area2: Property<S, f32>,
}

impl<S> Rectangle<S> {
    unsafe_pinned!(base: Item<S, f32>);
    unsafe_pinned!(color: Property<S, f32>);
    unsafe_pinned!(area2: Property<S, f32>);
}


struct Button<S> {
    background: Rectangle<S>,
    text: Text<S>,
    enabled: Property<S, bool>,
}

impl Button<S>


impl<S> Widget for Button<S> {



}


trait X<T>{
    type Y;
    fn foo()->();
    const AAA : u32 = 4;
}
impl X {
}

struct S<T>{
    member : Property<u32>
}

fn rectangle(s : impl State) -> impl Widget
*/





*/

/*

trait Item {
    fn width(self : Pin<&Self>) -> Pin<&Property<f32>>;
}




struct ItemBuilder<Base> {
    base: Base
}

impl<Base> std::ops::Deref for ItemBuilder<Base> {
    type Target = Base;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}



trait Button<S> {

}*/

/*
trait InitBindings {
    fn init(&mut self);
}

impl InitBindings for () {
    fn init(&mut self) {}
}

struct BindingStorage<S : InitBindings, T> {
    prev : S,
    binding: Binding<T>
}


impl<S : InitBindings, T> InitBindings for BindingStorage<S, T> {
    fn init(&mut self) {
        self.prev.init();
        self.binding
    }
}


struct Item {
    pub width: Property<f32>,
    pub height: Property<f32>,
    pub area1: Property<f32>,
}

impl Item {
    unsafe_pinned!(pub width: Property<f32>);
    unsafe_pinned!(pub height: Property<f32>);
    unsafe_pinned!(pub area1: Property<f32>);
}
*/




/*macro_rules! init_bindings {
    (in $item:ident : $item_type:ty { $($field:ident <$ret_type:ty> => $block:block )*) => {
        use ::core::pin::Pin;
        $({
            type ItemType = $item_type;
            fn call(this : Pin<&Binding<$ret_type>>, _: Pin<&Property<$ret_type>>) -> $ret_type {
                let ofst = ::memoffset::offset_of!(ItemType, $field) as isize;
                let raw = &*this as *const Binding<$ret_type> as *const u8;
                let raw = unsafe { raw.offset(-ofst } as *const ItemType;
                let $item : Pin<&ItemTyper> = unsafe { Pin::new_unchecked(&*raw) };
                $block
            }

        })*

    };

    /*($f:ident <$t:ty> in $s:ident : $S:ident $block:block ) => {
        {
            use ::core::pin::Pin;
            fn call(this : Pin<&Binding<$t>>, _: Pin<&Property<$t>>) -> $t {
                let raw = &*this as *const Binding<$t> as *const u8;
                let raw = unsafe { raw.offset(-(::memoffset::offset_of!($S, $f) as isize)) } as *const $S;
                let $s : Pin<&$S> = unsafe { Pin::new_unchecked(&*raw) };
                $block
            }
            let s : &mut Pin<&mut $S> = &mut $s;
            unsafe { s..$f.init(call) };
        }
    };*/
}*/
