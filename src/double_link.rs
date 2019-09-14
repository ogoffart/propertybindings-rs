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
