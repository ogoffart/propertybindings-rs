//! The new implementation of the property system.
//! Which requires no memory allocation and use Pin to stay safe

use core::cell::{Cell, RefCell};
use core::default::Default;
use core::marker::PhantomData;
use core::ops::DerefMut;
use core::pin::Pin;
use core::ptr::NonNull;

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

#[path = "double_link.rs"]
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
        core::mem::transmute::<Pin<&dyn NotificationReciever>, Pin<&'static dyn NotificationReciever>>(
            dep,
        )
    });
    CURRENT_PROPERTY.with(|cur_dep| {
        let mut m = cur_dep.borrow_mut();
        core::mem::swap(m.deref_mut(), &mut old);
    });
    let res = f();
    CURRENT_PROPERTY.with(|cur_dep| {
        let mut m = cur_dep.borrow_mut();
        core::mem::swap(m.deref_mut(), &mut old);
        //assert_eq!(old, Some(dep));
    });
    res
}

trait NotificationReciever {
    fn notify(self: Pin<&Self>, from: Pin<&dyn PropertyBase>);
    fn add_rev_dependency(self: Pin<&Self>, link: NonNull<DependencyNode>);
    fn needs_drop(&self) -> bool { false }
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
                true
            } else {
                false
            }
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
pub struct BindingStorage<B: ?Sized> {
    vtable: *const (),

    /// link to the list of properties upon which we depends
    rev_dep: Cell<double_link::Head<SenderList>>,
    /// link to the list of properties that depends on us
    // TODO: have static node, also no need for double link
    notify_dep: Cell<double_link::Head<NotifyList>>,

    // rev and rev_dep goes here
    binding: B,
}

impl<B> BindingStorage<B> {
    pub fn new<T>(binding: B) -> Self
    where
        B: Binding<T>,
    {
        let vtable = unsafe {
            core::mem::transmute::<&dyn Binding<T>, internal::TraitObject>(&binding).vtable
        };
        BindingStorage {
            vtable,
            rev_dep: Default::default(),
            notify_dep: Default::default(),
            binding,
        }
    }
}

struct BindingPtr<'a, T> {
    data: *const (),
    phantom: PhantomData<&'a T>,
}

impl<'a, T> BindingPtr<'a, T> {
    fn from(binding: Pin<&'a BindingStorage<dyn Binding<T> + 'a>>) -> Self {
        let binding: &BindingStorage<dyn Binding<T>> = binding.get_ref();
        let to = unsafe {
            core::mem::transmute::<&'a BindingStorage<dyn Binding<T>>, internal::TraitObject>(
                binding,
            )
        };
        // by construction  FIXME!  why is it not the case
        // debug_assert_eq!(binding.vtable, to.vtable);
        BindingPtr {
            data: to.data,
            phantom: PhantomData,
        }
    }
    unsafe fn from_raw(data: *const ()) -> Self {
        BindingPtr {
            data,
            phantom: PhantomData,
        }
    }
    fn into_raw(self) -> *const () {
        self.data
    }
    fn as_ref(&self) -> Pin<&'a dyn Binding<T>> {
        #[allow(clippy::cast_ptr_alignment)] // that's the actual type, and the alignment is correct
        let vtable = unsafe { *(self.data as *const *const ()) };
        let storage = unsafe {
            core::mem::transmute::<internal::TraitObject, &'a BindingStorage<dyn Binding<T>>>(
                internal::TraitObject {
                    data: self.data,
                    vtable,
                },
            )
        };
        debug_assert_eq!(vtable, storage.vtable);
        unsafe { Pin::new_unchecked(&storage.binding) }
    }

    unsafe fn drop_binding(self) {
        #[allow(clippy::cast_ptr_alignment)] // that's the actual type, and the alignment is correct
        let vtable = *(self.data as *const *const ());
        let storage = core::mem::transmute::<
            internal::TraitObject,
            &'a BindingStorage<dyn Binding<T>>,
        >(internal::TraitObject {
            data: self.data,
            vtable,
        });
        Box::from_raw(
            storage as *const BindingStorage<dyn Binding<T>> as *mut BindingStorage<dyn Binding<T>>,
        );
    }

    fn storage(&self) -> &'a BindingStorage<dyn Binding<T>> {
        let vtable = unsafe { *(self.data as *const *const ()) };
        let storage = unsafe {
            core::mem::transmute::<internal::TraitObject, &'a BindingStorage<dyn Binding<T>>>(
                internal::TraitObject {
                    data: self.data,
                    vtable,
                },
            )
        };
        debug_assert_eq!(vtable, storage.vtable);
        storage
    }
}

impl<'a, T> core::ops::Deref for BindingPtr<'a, T> {
    type Target = BindingStorage<dyn Binding<T>>;
    fn deref(&self) -> &Self::Target {
        self.storage()
    }
}

#[repr(C)]
pub struct Property<T> {
    // if value & 1 { BindingPtr<T> } else { double_link::Head<NotifyList> }
    // if value & 0b11, it needs to be dropped
    internal: Cell<usize>,
    phantom: core::marker::PhantomPinned,
    value: core::cell::UnsafeCell<T>,
}

//Private API's
impl<T> Property<T> {
    fn binding<'a>(&'a self) -> Option<BindingPtr<'a, T>> {
        let v = self.internal.get();
        if v & 0b1 == 0b1 {
            let v = v & (!0b11);
            Some(unsafe { BindingPtr::<'a, T>::from_raw(v as *const _) })
        } else {
            None
        }
    }

    fn notify_dep<'a>(&'a self) -> &'a Cell<double_link::Head<NotifyList>> {
        self.binding()
            .map(|b| &b.storage().notify_dep)
            .unwrap_or_else(|| unsafe {
                core::mem::transmute::<_, &'a Cell<double_link::Head<NotifyList>>>(&self.internal)
            })
    }

    fn remove_binding(self : Pin<&Self>) {
        let v = self.internal.get();
        if let Some(b) = self.binding() {
            self.internal.set(0);
            unsafe {
                (*self.notify_dep().as_ptr()).swap(&mut *b.notify_dep.as_ptr());
                if v & 0b11 == 0b11 {
                    b.drop_binding();
                }
            }
        }
    }

}

impl<T> Drop for Property<T> {
    fn drop(&mut self) {
        unsafe {
            Pin::new_unchecked(&*self).remove_binding();
            let head_ptr = &self.internal as *const _;
            let head : double_link::Head<NotifyList> =  core::ptr::read(head_ptr as *const _);
            core::mem::drop(head);
        }
    }
}

impl<T : Default> Default for Property<T> {
    fn default() -> Self {
        Property {
            internal: Cell::new(0),
            phantom: core::marker::PhantomPinned,
            value: Default::default()
        }
    }
}

impl<T: Clone> Property<T> {
    pub fn set(self: Pin<&Self>, t: T) {
        self.remove_binding();
        unsafe { *self.value.get() = t }
        self.update_dependencies();
    }

    pub fn get(self: Pin<&Self>) -> T {
        self.accessed();
        unsafe { &*self.value.get() }.clone()
    }
}

impl<T> Property<T> {
    pub fn set_binding<'a>(self: Pin<&'a Self>, b: Pin<&'a BindingStorage<dyn Binding<T> + 'a>>) {
        let b = BindingPtr::from(b);
        unsafe {
            (*self.notify_dep().as_ptr()).swap(&mut *b.notify_dep.as_ptr());
            let v = b.into_raw() as usize;
            assert!(v & 0b11 == 0);
            let v = v | 1usize;
            if self.internal.get() == v {
                return;
            };
            self.remove_binding();
            self.internal.set(v);
        }
        self.notify(self);
    }

    pub fn set_binding_owned<'a, B: Binding<T> + 'a>(self: Pin<&Self>, b: B) {
        let b : Box<BindingStorage<dyn Binding<T> + 'a>> = Box::new(BindingStorage::new(b));
        unsafe {
            (*self.notify_dep().as_ptr()).swap(&mut *b.notify_dep.as_ptr());
            self.remove_binding();
            let ptr = Box::into_raw(b);
            let v = core::mem::transmute::<_, internal::TraitObject>(ptr).data as usize;
            assert!(v & 0b11 == 0);
            self.internal.set(v | 0b11usize);
        }
        self.notify(self);
    }

    fn update_dependencies(self: Pin<&Self>) {
        let mut v = Default::default();
        unsafe { &mut *self.notify_dep().as_ptr() }.swap(&mut v);
        for d in v {
            let elem = d.elem.clone();
            core::mem::drop(d); // One need to drop it to remove it from the rev list before calling update.
            unsafe {
                Pin::new_unchecked(elem.as_ref()).notify(self);
            }
        }
    }
}

impl<T> NotificationReciever for Property<T> {
    fn notify(self: Pin<&Self>, _from: Pin<&dyn PropertyBase>) {
        if let Some(b) = self.binding() {
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
    fn add_rev_dependency(self: Pin<&Self>, link: NonNull<DependencyNode>) {
        unsafe {
            self.binding()
                .map(|b| (&mut *b.rev_dep.as_ptr()).append(link));
        }
    }
}

impl<T> PropertyBase for Property<T> {
    fn add_dependency(&self, link: NonNull<DependencyNode>) {
        unsafe {
            (&mut *self.notify_dep().as_ptr()).append(link);
        }
    }
}

pub struct ChangeEvent<F: Fn() + ?Sized> {
    list: Cell<double_link::Head<NotifyList>>,
    func: F,
}

impl<F: Fn()> ChangeEvent<F> {
    pub fn new(func: F) -> Self {
        ChangeEvent {
            func,
            list: Default::default(),
        }
    }

    pub fn listen<T>(self: Pin<&Self>, p: Pin<&Property<T>>) {
        self.listen_impl(p)
    }

    fn listen_impl(self: Pin<&Self>, p: Pin<&dyn PropertyBase>) {
        // cast away lifetime because we register the destructor anyway
        let s = unsafe {
            core::mem::transmute::<&dyn NotificationReciever, &(dyn NotificationReciever + 'static)>(
                &*self,
            )
        };
        let b = Box::new(DependencyNode::new(s.into()));
        let b = unsafe { NonNull::new_unchecked(Box::into_raw(b)) };
        unsafe { (*self.list.as_ptr()).append(b) };
        p.as_ref().add_dependency(b);
    }
}

impl<F: Fn()> NotificationReciever for ChangeEvent<F> {
    fn notify(self: Pin<&Self>, from: Pin<&dyn PropertyBase>) {
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
    /*
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
    }*/

    #[test]
    fn test_property() {
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
            item.project_ref().height.get() * item.project_ref().width.get()
        });

        #[pin_project::pin_project]
        #[derive(Default)]
        struct Item {
            #[pin]
            pub width: Property<f32>,
            #[pin]
            pub height: Property<f32>,
            #[pin]
            pub area: Property<f32>,
        }

        let i = Item::default();
        pin_utils::pin_mut!(i);
        let i = i.as_ref();
        let area_binding = AreaBinding::new(i);
        let area_binding = BindingStorage::new(area_binding);
        pin_utils::pin_mut!(area_binding);
        i.project_ref().height.set(12.);
        i.project_ref().width.set(8.);
        i.project_ref().area.set_binding(area_binding.as_ref());
        assert_eq!(i.project_ref().area.get(), 12. * 8.);
        i.project_ref().width.set(4.);
        assert_eq!(i.project_ref().area.get(), 12. * 4.);

        make_binding!(struct AreaBinding2<'a> : "Binding<f32>" f32 => |item : Pin<&'a Item> | {
            item.project_ref().height.get() + item.project_ref().width.get()
        });
        i.project_ref().area.set_binding_owned(AreaBinding2::new(i));
        assert_eq!(i.project_ref().area.get(), 12. + 4.);
        i.project_ref().height.set(8.);
        assert_eq!(i.project_ref().area.get(), 8. + 4.);
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
