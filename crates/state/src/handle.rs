//! Observable state primitives.
//!
//! A [`Handle`] is a single observed value: clone to share it, [`Handle::subscribe`]
//! to hear about changes, [`Handle::set`] / [`Handle::update`] to change it. It is
//! deliberately a `Rc<RefCell<_>>` rather than a thread-safe cell: the client runs
//! on one thread, and native tests drive the same code on one thread.

use std::cell::{Cell, RefCell};
use std::fmt;
use std::rc::Rc;

/// Identifier of a subscription, used for unlinking.
type SubscriptionId = u64;

/// Callback run when the observed value changes.
type Callback = Rc<dyn Fn()>;

/// A single observed value with change notification.
pub struct Handle<T> {
    inner: Rc<Inner<T>>,
}

struct Inner<T> {
    value: RefCell<T>,
    subscribers: RefCell<Vec<(SubscriptionId, Callback)>>,
    next_subscription: Cell<SubscriptionId>,
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T> Handle<T> {
    /// Creates a handle holding `initial`.
    #[must_use]
    pub fn new(initial: T) -> Self {
        Self {
            inner: Rc::new(Inner {
                value: RefCell::new(initial),
                subscribers: RefCell::new(Vec::new()),
                next_subscription: Cell::new(0),
            }),
        }
    }

    /// Registers `callback` to run after every change.
    ///
    /// Dropping the returned [`Subscription`] unregisters it.
    pub fn subscribe<F>(&self, callback: F) -> Subscription
    where
        T: 'static,
        F: Fn() + 'static,
    {
        let id = self.inner.next_subscription.get();
        self.inner.next_subscription.set(id + 1);
        self.inner
            .subscribers
            .borrow_mut()
            .push((id, Rc::new(callback)));

        let inner = Rc::clone(&self.inner);
        Subscription {
            unsubscribe: Some(Box::new(move || {
                inner
                    .subscribers
                    .borrow_mut()
                    .retain(|(other, _)| *other != id);
            })),
        }
    }

    fn notify(&self) {
        // Collect first: a callback may itself subscribe or unsubscribe.
        let callbacks: Vec<Callback> = self
            .inner
            .subscribers
            .borrow()
            .iter()
            .map(|(_, callback)| Rc::clone(callback))
            .collect();
        for callback in callbacks {
            callback();
        }
    }
}

impl<T: Clone> Handle<T> {
    /// Returns a copy of the current value.
    pub fn get(&self) -> T {
        self.inner.value.borrow().clone()
    }

    /// Runs `f` against the current value without cloning it.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        f(&self.inner.value.borrow())
    }

    /// Replaces the value and notifies subscribers.
    pub fn set(&self, value: T) {
        *self.inner.value.borrow_mut() = value;
        self.notify();
    }

    /// Mutates the value in place and notifies subscribers.
    pub fn update<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = {
            let mut value = self.inner.value.borrow_mut();
            f(&mut value)
        };
        self.notify();
        result
    }
}

impl<T: fmt::Debug> fmt::Debug for Handle<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Handle")
            .field(&*self.inner.value.borrow())
            .finish()
    }
}

/// Keeps a subscription alive; dropping it unregisters the callback.
#[derive(Default)]
pub struct Subscription {
    unsubscribe: Option<Box<dyn FnOnce()>>,
}

impl Subscription {
    /// Unregisters the callback now instead of at drop.
    pub fn cancel(mut self) {
        if let Some(unsubscribe) = self.unsubscribe.take() {
            unsubscribe();
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(unsubscribe) = self.unsubscribe.take() {
            unsubscribe();
        }
    }
}

impl fmt::Debug for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Subscription")
            .field("active", &self.unsubscribe.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_notifies_subscribers() {
        let handle = Handle::new(0_u32);
        let calls = Rc::new(Cell::new(0_u32));
        let counted = Rc::clone(&calls);
        let _subscription = handle.subscribe(move || counted.set(counted.get() + 1));

        handle.set(1);
        handle.update(|value| *value += 1);

        assert_eq!(handle.get(), 2);
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn dropped_subscription_stops_notifications() {
        let handle = Handle::new(0_u32);
        let calls = Rc::new(Cell::new(0_u32));
        let counted = Rc::clone(&calls);
        let subscription = handle.subscribe(move || counted.set(counted.get() + 1));

        handle.set(1);
        drop(subscription);
        handle.set(2);

        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn a_handler_may_unsubscribe_itself() {
        let handle = Handle::new(0_u32);
        let observed = Rc::new(Cell::new(0_u32));

        let slot = Rc::new(RefCell::new(None::<Subscription>));
        let sink = Rc::clone(&observed);
        let holder = Rc::clone(&slot);
        let subscription = handle.subscribe(move || {
            sink.set(sink.get() + 1);
            if let Some(subscription) = holder.borrow_mut().take() {
                subscription.cancel();
            }
        });
        *slot.borrow_mut() = Some(subscription);

        handle.set(1);
        handle.set(2);

        assert_eq!(observed.get(), 1);
    }
}
