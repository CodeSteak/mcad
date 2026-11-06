//! [`ShapeCell`]: a `RefCell` that several threads can share. A thread that borrows it owns it
//! until its last borrow ends; other threads wait. Within the owning thread the `RefCell`'s
//! rules hold as before (any number of shared borrows, or one mutable borrow; a conflict
//! panics), so code that borrows a shape twice on one thread (a Boolean of a shape with
//! itself) behaves as with a plain `RefCell`, where a `Mutex` would deadlock.
//!
//! Shapes are shared between the engine's worker threads only where two nodes use the same
//! result (identical subtrees); the lock serialises their queries, which take `&mut Brep` for
//! solidcore's caches.

use std::cell::{Ref, RefCell, RefMut};
use std::mem::ManuallyDrop;
use std::ops::{Deref, DerefMut};
use std::sync::{Condvar, Mutex};
use std::thread::ThreadId;

pub struct ShapeCell<T> {
    /// The owning thread and the number of its live borrows.
    owner: Mutex<(Option<ThreadId>, usize)>,
    free: Condvar,
    cell: RefCell<T>,
}

// SAFETY: `cell` is only touched by the thread recorded in `owner` (`acquire` waits until no
// other thread owns the cell, `release` gives it up when its last borrow ends), so the
// `RefCell` is never used by two threads at once; `T: Send` lets it move between them.
unsafe impl<T: Send> Sync for ShapeCell<T> {}

impl<T> ShapeCell<T> {
    pub fn new(v: T) -> Self {
        ShapeCell { owner: Mutex::new((None, 0)), free: Condvar::new(), cell: RefCell::new(v) }
    }

    fn acquire(&self) {
        let me = std::thread::current().id();
        let mut o = self.owner.lock().unwrap_or_else(|e| e.into_inner());
        while o.0.is_some_and(|t| t != me) {
            o = self.free.wait(o).unwrap_or_else(|e| e.into_inner());
        }
        o.0 = Some(me);
        o.1 += 1;
    }

    fn release(&self) {
        let mut o = self.owner.lock().unwrap_or_else(|e| e.into_inner());
        o.1 -= 1;
        if o.1 == 0 {
            o.0 = None;
            self.free.notify_one();
        }
    }

    pub fn borrow(&self) -> CellRef<'_, T> {
        self.acquire();
        CellRef { r: ManuallyDrop::new(self.cell.borrow()), cell: self }
    }

    pub fn borrow_mut(&self) -> CellMut<'_, T> {
        self.acquire();
        CellMut { r: ManuallyDrop::new(self.cell.borrow_mut()), cell: self }
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for ShapeCell<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ShapeCell")
    }
}

pub struct CellRef<'a, T> {
    r: ManuallyDrop<Ref<'a, T>>,
    cell: &'a ShapeCell<T>,
}

impl<T> Deref for CellRef<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.r
    }
}

impl<T> Drop for CellRef<'_, T> {
    fn drop(&mut self) {
        // SAFETY: dropped once, here; the borrow ends before the cell is released
        unsafe { ManuallyDrop::drop(&mut self.r) };
        self.cell.release();
    }
}

pub struct CellMut<'a, T> {
    r: ManuallyDrop<RefMut<'a, T>>,
    cell: &'a ShapeCell<T>,
}

impl<T> Deref for CellMut<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.r
    }
}

impl<T> DerefMut for CellMut<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.r
    }
}

impl<T> Drop for CellMut<'_, T> {
    fn drop(&mut self) {
        // SAFETY: dropped once, here; the borrow ends before the cell is released
        unsafe { ManuallyDrop::drop(&mut self.r) };
        self.cell.release();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Nested shared borrows on one thread work (as with a `RefCell`); another thread waits
    /// until they end.
    #[test]
    fn reentrant_on_one_thread_exclusive_across_threads() {
        let c = Arc::new(ShapeCell::new(0u32));
        let a = c.borrow();
        let b = c.borrow();
        assert_eq!(*a + *b, 0);
        let c2 = c.clone();
        let t = std::thread::spawn(move || {
            *c2.borrow_mut() += 1;
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(*a, 0, "the other thread waits while this one borrows");
        drop((a, b));
        t.join().unwrap();
        assert_eq!(*c.borrow(), 1);
    }
}
