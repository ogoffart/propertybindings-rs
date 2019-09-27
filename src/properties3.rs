#![allow(dead_code)]

use core::cell::{Cell, RefCell};
use core::default::Default;
use core::ptr::NonNull;
use core::pin::Pin;
use std::ops::DerefMut;
use std::marker::PhantomData;


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
    fn notify(self : Pin<&Self>, from: Pin<&dyn PropertyBase>);
    fn add_rev_dependency(self: Pin<&Self>, link: NonNull<DependencyNode>);
}

trait PropertyBase {
    fn add_dependency(&self, link: NonNull<DependencyNode>);
//    fn update_dependencies(&self);

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
    fn call(self: Pin<&Self>) -> T;
}

impl<F, T> Binding<T> for F
where
    F: Fn() -> T,
{
    fn call(self: Pin<&Self>) -> T {
        (*self.get_ref())()
    }
}

#[repr(C)]
pub struct BindingStorage<B : ?Sized> {
    vtable: *const (),

    /// link to the list of properties upon which we depends
    rev_dep: Cell<double_link::Head<SenderList>>,
    /// link to the list of properties that depends on us
    // TODO: have static node, also no need for double link
    notify_dep: Cell<double_link::Head<NotifyList>>,


    // rev and rev_dep goes here
    binding : B
}

impl<B> BindingStorage<B> {
    pub fn new<T>(binding : B) -> Self where B: Binding<T> {
        let vtable = unsafe {std::mem::transmute::<&dyn Binding<T>, internal::TraitObject>(&binding).vtable };
        BindingStorage {
            vtable,
            rev_dep: Default::default(),
            notify_dep: Default::default(),
            binding
        }
    }
}

struct BindingPtr<'a, T> {
    data: *const (),
    phantom: PhantomData<&'a T>
}

impl<'a, T> BindingPtr<'a, T> {
    fn from(binding : Pin<&'a BindingStorage<dyn Binding<T> + 'a>>) -> Self {
        let binding : &BindingStorage<dyn Binding<T>> = binding.get_ref();
        let to = unsafe {
            std::mem::transmute::<&'a BindingStorage<dyn Binding<T>>,
                internal::TraitObject>(binding)
        };
        // by construction  FIXME!  why is it not the case
        // debug_assert_eq!(binding.vtable, to.vtable);
        BindingPtr { data: to.data, phantom: PhantomData  }
    }
    unsafe fn from_raw(data: *const ()) -> Self {
        BindingPtr{ data, phantom: PhantomData }
    }
    fn into_raw(&self) -> *const () { self.data }
    fn as_ref(&self) -> Pin<&'a dyn Binding<T>> {
        let vtable = unsafe { *(self.data as *const *const()) };
        let storage = unsafe {
            std::mem::transmute::<
                internal::TraitObject,
                &'a BindingStorage<dyn Binding<T>>
            >(internal::TraitObject{ data: self.data, vtable })
        };
        debug_assert_eq!(vtable, storage.vtable);
        unsafe { Pin::new_unchecked(&storage.binding) }
    }

    unsafe fn drop_binding(self) {
        let vtable = *(self.data as *const *const());
        let storage = std::mem::transmute::<
                internal::TraitObject,
                &'a BindingStorage<dyn Binding<T>>
            >(internal::TraitObject{ data: self.data, vtable });
        Box::from_raw(storage as *const BindingStorage<dyn Binding<T>> as *mut BindingStorage<dyn Binding<T>>);
    }

    fn storage(&self) -> &'a BindingStorage<dyn Binding<T>> {
        let vtable = unsafe { *(self.data as *const *const()) };
        let storage = unsafe {
            std::mem::transmute::<
                internal::TraitObject,
                &'a BindingStorage<dyn Binding<T>>
            >(internal::TraitObject{ data: self.data, vtable })
        };
        debug_assert_eq!(vtable, storage.vtable);
        storage
    }
}

impl<'a, T> std::ops::Deref for BindingPtr<'a, T> {
    type Target = BindingStorage<dyn Binding<T>>;
    fn deref(&self) -> &Self::Target {
        self.storage()
    }
}


#[derive(Default)]
struct PropertyInternal<T> {
    // if value & 1 { BindingPtr<T> } else { double_link::Head<NotifyList> }
    // if value & 0b11, it needs to be dropped
    value: core::cell::Cell<usize>,
    phantom: core::marker::PhantomData<(T, std::marker::PhantomPinned)>
}

impl<T> PropertyInternal<T> {
    unsafe fn binding<'a>(&'a self) -> Option<BindingPtr<'a, T>> {
        let v = self.value.get();
        if v & 0b1 == 0b1 {
            let v = v & (!0b11);
            Some(BindingPtr::<T>::from_raw(v as *const _))
        } else {
            None
        }
    }

    unsafe fn notify_dep<'a>(&'a self) -> &'a Cell<double_link::Head<NotifyList>> {
        self.binding().map(|b| &b.storage().notify_dep).unwrap_or_else(||
            std::mem::transmute::<_, &'a Cell<double_link::Head<NotifyList>>>(&self.value))
    }

    unsafe fn set_binding<'a>(&'a self, b: Pin<&'a BindingStorage<dyn Binding<T> + 'a>>) {
        let b = BindingPtr::from(b);
        (*self.notify_dep().as_ptr()).swap(&mut *b.notify_dep.as_ptr());
        let v = std::mem::transmute::<BindingPtr<T>, usize>(b);
        assert!(v & 0b11 == 0);
        let v = v | 1usize;
        if self.value.get() == v { return };
        self.remove_binding();
        self.value.set(v);
    }

    unsafe fn remove_binding<'a>(&'a self) {
        let v = self.value.get();
        if let Some(b) = self.binding() {
            self.value.set(0);
            (*self.notify_dep().as_ptr()).swap(&mut *b.notify_dep.as_ptr());
            if v & 0b11 == 0b11 {
                b.drop_binding();
            }
        }
    }

    unsafe fn set_binding_box<'a>(&'a self, b: Box<BindingStorage<dyn Binding<T> + 'a>>) {
        (*self.notify_dep().as_ptr()).swap(&mut *b.notify_dep.as_ptr());
        self.remove_binding();
        let ptr = Box::into_raw(b);
        let v = std::mem::transmute::<_, internal::TraitObject>(ptr).data as usize;
        assert!(v & 0b11 == 0);
        self.value.set(v | 0b11usize);
    }
}

impl<T> Drop for PropertyInternal<T> {
    fn drop(&mut self) {
        unsafe { self.remove_binding(); }
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
    pub fn set_binding<'a>(self : Pin<&'a Self>, b: Pin<&'a BindingStorage<dyn Binding<T> + 'a>>) {
        unsafe { self.internal.set_binding(b) };
        self.notify(self);
    }

    pub fn set_binding_owned<'a, B : Binding<T> + 'a>(self : Pin<&Self>, b: B) {
        unsafe { self.internal.set_binding_box(Box::new(BindingStorage::new(b))) };
        self.notify(self);
    }

    pub fn get(self : Pin<&Self>) -> T {
        self.accessed();
        unsafe { &*self.value.get() }.clone()
    }
}

impl<T> Property<T> {
    fn update_dependencies(self : Pin<&Self>) {
        let mut v = Default::default();
        unsafe { &mut *self.internal.notify_dep().as_ptr() }.swap(&mut v);
        for d in v {
            let elem = d.elem.clone();
            std::mem::drop(d); // One need to drop it to remove it from the rev list before calling update.
            unsafe { Pin::new_unchecked(elem.as_ref()).notify(self); }
        }
    }
}

impl<T> NotificationReciever for Property<T> {
    fn notify(self : Pin<&Self>, _from : Pin<&dyn PropertyBase>) {
        if let Some(b) = unsafe { self.internal.binding() } {
            /*if self.updating.get() {
                panic!("Circular dependency found : {}", self.description());
            }
            self.updating.set(true);*/
            // clear dependency
            unsafe { &mut *b.rev_dep.as_ptr() }.clear();

            let val = run_with_current(self, || b.as_ref().call());
            // FIXME: check that the property does actualy change
            unsafe { *self.value.get() = val }
            self.update_dependencies();
            //self.updating.set(false);
        }
    }
    fn add_rev_dependency(self : Pin<&Self>, link: NonNull<DependencyNode>) {
        unsafe {
            self.internal.binding().map(|b| (&mut *b.rev_dep.as_ptr()).append(link) );
        }
    }

}

impl<T> PropertyBase for Property<T> {
    fn add_dependency(&self, link: NonNull<DependencyNode>) {
        unsafe {
            (&mut *self.internal.notify_dep().as_ptr()).append(link);
        }
    }
}


pub struct ChangeEvent<F: Fn() + ?Sized> {
    list: Cell<double_link::Head<NotifyList>>,
    func: F,
}

impl<F: Fn()> ChangeEvent<F> {
    pub fn new(func: F) -> Self {
        ChangeEvent { func, list: Default::default() }
    }

    pub fn listen<T>(self: Pin<&Self>, p : Pin<&Property<T>>) {
        self.listen_impl(p)
    }

    fn listen_impl(self: Pin<&Self>, p : Pin<&dyn PropertyBase>) {
        // cast away lifetime because we register the destructor anyway
        let s = unsafe { std::mem::transmute::<&dyn NotificationReciever,
            &(dyn NotificationReciever + 'static)>(&*self) };
        let b = Box::new(DependencyNode::new(s.into()));
        let b = unsafe { NonNull::new_unchecked(Box::into_raw(b)) };
        unsafe { (*self.list.as_ptr()).append(b) };
        p.as_ref().add_dependency(b);

    }
}

impl<F: Fn()> NotificationReciever for ChangeEvent<F>
{
    fn notify(self : Pin<&Self>, from : Pin<&dyn PropertyBase>) {
        (self.func)();
        // re-add the signal
        self.listen_impl(from)
    }


    fn add_rev_dependency(self: Pin<&Self>, _link: NonNull<DependencyNode>) {
        unreachable!();
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

/*
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

        */

        macro_rules! make_binding {
            (struct $name:ident $(< $($lt:lifetime),* >)? : $st:literal $type:ty =>
                | $state:ident : $state_ty:ty | $block:block ) => {
                struct $name $(<$($lt),*>)* ($state_ty,);
                impl $(<$($lt)*>)* Binding<f32> for $name $(<$($lt)*>)*{
                    fn call(self: ::core::pin::Pin<&Self>) -> $type {
                        let $state = unsafe { ::core::pin::Pin::map_unchecked(self, |s| &s.0) };
                        $block
                    }
                }
                impl $(<$($lt)*>)* $name $(<$($lt)*>)* {
                    fn new($state : $state_ty) -> Self {
                        $name($state,)
                    }
                }
            };
        }

        make_binding!(struct AreaBinding<'a> : "Binding<f32>" f32 => |item : Pin<&'a Item> | {
            item.height().get() * item.width().get()
        });

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

        let i = Item::default();
        pin_utils::pin_mut!(i);
        let i = i.as_ref();
        let area_binding = AreaBinding::new(i);
        let area_binding = BindingStorage::new(area_binding);
        pin_utils::pin_mut!(area_binding);
        i.height().set(12.);
        i.width().set(8.);
        i.area().set_binding(area_binding.as_ref());
        assert_eq!(i.area().get(), 12.*8.);
        i.width().set(4.);
        assert_eq!(i.area().get(), 12.*4.);

        make_binding!(struct AreaBinding2<'a> : "Binding<f32>" f32 => |item : Pin<&'a Item> | {
            item.height().get() + item.width().get()
        });
        i.area().set_binding_owned(AreaBinding2::new(i));
        assert_eq!(i.area().get(), 12.+4.);
        i.height().set(8.);
        assert_eq!(i.area().get(), 8.+4.);


        /*{
            let item = std::rc::Rc::pin(Item::default());
            make_binding!(struct AreaBinding2<'a> : "Binding<f32>" f32 => |item : Pin<Rc<Item>> | {
                item.as_ref().height().get() + item.as_ref().width().get()
            });
            i.area().set_binding_owned(ThinBox::pin(AreaBinding2::new(i)));
        }*/
    }



    #[test]
    fn test_notify() {
        let x = Cell::new(0);
        let bar = Property::default();
        let foo = Property::default();
        pin_utils::pin_mut!(bar);
        pin_utils::pin_mut!(foo);
        let bar = bar.as_ref();
        let foo = foo.as_ref();
        bar.set(2);
        foo.set(2);
        let e = ChangeEvent::new(|| x.set(x.get() + 1));
        pin_utils::pin_mut!(e);
        e.as_ref().listen(foo);
        foo.set(3);
        assert_eq!(x.get(), 1);
        foo.set(45);
        assert_eq!(x.get(), 2);
        foo.set_binding_owned(|| bar.get());
        assert_eq!(x.get(), 3);
        bar.set(8);
        assert_eq!(x.get(), 4);
    }
}

